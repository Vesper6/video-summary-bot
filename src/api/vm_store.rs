//! VM 配置持久化（参考 tenbox `vm.json` + `runtime_state.json`）。
//!
//! `start()` 在 hypervisor 与 guest 镜像可用时走真实 [`crate::vmm::boot::run_boot`]
//! 管线（tokio 后台任务），串口日志通过 [`crate::guest_log`] 钩子进入控制台缓冲；
//! 否则退回演示模式（仅状态机 + 提示日志）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::vmm::{VmConfig, VmState};

const VM_MANIFEST: &str = "vm.json";
const RUNTIME_STATE: &str = "runtime_state.json";
const CONSOLE_MAX_LINES: usize = 500;

/// API 返回的 VM 摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmSummary {
    /// VM 名称
    pub name: String,
    /// 当前运行状态
    pub state: VmState,
    /// vCPU 数量
    pub cpus: u8,
    /// 内存大小（MB）
    pub memory_mb: u32,
    /// 磁盘大小（GB）
    pub disk_gb: u32,
    /// 内核启动命令行
    pub cmdline: Option<String>,
    /// 启动时间（Unix 时间戳，未运行为 None）
    pub started_at: Option<i64>,
    /// 控制台输出行
    pub console_lines: Vec<String>,
}

/// 创建 VM 请求。
#[derive(Debug, Deserialize)]
pub struct CreateVmRequest {
    /// VM 名称
    pub name: String,
    /// vCPU 数量（默认 2）
    #[serde(default = "default_cpus")]
    pub cpus: u8,
    /// 内存大小（MB，默认 2048）
    #[serde(default = "default_memory")]
    pub memory_mb: u32,
    /// 磁盘大小（GB，默认 10）
    #[serde(default = "default_disk")]
    pub disk_gb: u32,
    /// 内核启动命令行（缺省用内置默认值）
    pub cmdline: Option<String>,
    /// 自定义内核路径（默认用 assets/kernels/vmlinuz）
    #[serde(default)]
    pub kernel: Option<PathBuf>,
    /// 自定义 initramfs 路径
    #[serde(default)]
    pub initramfs: Option<PathBuf>,
    /// 自定义 rootfs 路径
    #[serde(default)]
    pub rootfs: Option<PathBuf>,
}

/// 更新 VM 请求（运行中不可改）。
#[derive(Debug, Default, Deserialize)]
pub struct UpdateVmRequest {
    /// 新 vCPU 数量
    pub cpus: Option<u8>,
    /// 新内存大小（MB）
    pub memory_mb: Option<u32>,
    /// 新磁盘大小（GB）
    pub disk_gb: Option<u32>,
    /// 新内核启动命令行（空串表示清除）
    pub cmdline: Option<String>,
}

fn default_cpus() -> u8 {
    2
}
fn default_memory() -> u32 {
    2048
}
fn default_disk() -> u32 {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct RuntimeState {
    state: VmState,
    started_at: i64,
    #[serde(default)]
    console_lines: Vec<String>,
}

/// 内存中的控制台缓冲（按 VM 名索引）。
type ConsoleMap = HashMap<String, Vec<String>>;

/// VM 存储：`root/<name>/vm.json` + `runtime_state.json`。
///
/// 可 `Clone`（内部为共享句柄），boot 后台任务持有克隆用于状态回写。
#[derive(Clone)]
pub struct VmStore {
    root: PathBuf,
    consoles: Arc<RwLock<ConsoleMap>>,
    /// 当前正在真实 boot 的 VM 名（guest_log / guest_control 钩子是全局的，
    /// 因此同一时刻只允许一个真实 boot）。
    active_boot: Arc<RwLock<Option<String>>>,
}

impl VmStore {
    /// 创建 VM 存储（自动创建根目录）。
    pub fn new(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        std::fs::create_dir_all(&root)
            .map_err(|e| Error::Config(format!("create vm store: {e}")))?;
        Ok(Self {
            root,
            consoles: Arc::new(RwLock::new(HashMap::new())),
            active_boot: Arc::new(RwLock::new(None)),
        })
    }

    /// 默认存储根目录（`./data/vms`）。
    pub fn default_path() -> PathBuf {
        PathBuf::from("./data/vms")
    }

    fn vm_dir(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    fn push_console(&self, name: &str, line: impl Into<String>) {
        let line = line.into();
        let mut map = self.consoles.write();
        let buf = map.entry(name.to_string()).or_default();
        buf.push(line);
        if buf.len() > CONSOLE_MAX_LINES {
            let drain = buf.len() - CONSOLE_MAX_LINES;
            buf.drain(0..drain);
        }
    }

    /// 当前控制台行（本次进程内的实时缓冲优先，否则用上次持久化的）。
    fn console_lines(&self, name: &str) -> Vec<String> {
        let live = self.consoles.read().get(name).cloned().unwrap_or_default();
        if !live.is_empty() {
            return live;
        }
        self.load_runtime(name).console_lines
    }

    /// 增量读取控制台行（供 SSE 使用）：返回 `from` 之后的新行与新游标。
    pub fn console_tail(&self, name: &str, from: usize) -> Result<(Vec<String>, usize)> {
        if !self.vm_dir(name).exists() {
            return Err(Error::Config(format!("VM '{name}' not found")));
        }
        let lines = self.console_lines(name);
        let total = lines.len();
        // 缓冲区滚动（超过上限被裁剪）时 from 可能大于 total，此时从头补发
        let start = if from > total { 0 } else { from };
        Ok((lines[start..].to_vec(), total))
    }

    /// 是否有真实 boot 任务在跑（演示模式不算）。
    pub fn is_boot_active(&self, name: &str) -> bool {
        self.active_boot.read().as_deref() == Some(name)
    }

    /// 列出所有 VM（按名称排序）。
    pub fn list(&self) -> Result<Vec<VmSummary>> {
        let mut out = Vec::new();
        let entries = std::fs::read_dir(&self.root)
            .map_err(|e| Error::Config(format!("read vm store: {e}")))?;
        for entry in entries {
            let entry = entry.map_err(|e| Error::Config(format!("read dir entry: {e}")))?;
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Ok(summary) = self.get(&name) {
                out.push(summary);
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    /// 获取单个 VM 的摘要信息。
    pub fn get(&self, name: &str) -> Result<VmSummary> {
        let spec = self.load_spec(name)?;
        let runtime = self.load_runtime(name);
        Ok(VmSummary {
            name: spec.name,
            state: runtime.state,
            cpus: spec.cpus,
            memory_mb: spec.memory_mb,
            disk_gb: spec.disk_gb,
            cmdline: spec.cmdline,
            started_at: if runtime.started_at > 0 {
                Some(runtime.started_at)
            } else {
                None
            },
            console_lines: self.console_lines(name),
        })
    }

    /// 创建新 VM（校验名称合法性与重名）。
    pub fn create(&self, req: CreateVmRequest) -> Result<VmSummary> {
        if req.name.is_empty() {
            return Err(Error::Config("VM name cannot be empty".into()));
        }
        if !req
            .name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(Error::Config(
                "VM name may only contain letters, digits, '-' and '_'".into(),
            ));
        }

        let dir = self.vm_dir(&req.name);
        if dir.exists() {
            return Err(Error::Config(format!("VM '{}' already exists", req.name)));
        }

        let mut spec = VmConfig::new(&req.name);
        spec.cpus = req.cpus;
        spec.memory_mb = req.memory_mb;
        spec.disk_gb = req.disk_gb;
        spec.kernel = req.kernel;
        spec.initramfs = req.initramfs;
        spec.rootfs = req.rootfs;
        spec.cmdline = req.cmdline.or_else(|| {
            Some(
                "earlyprintk=ttyS0,115200 console=ttyS0,115200 root=/dev/vda rw init=/etc/init.d/rcS"
                    .to_string(),
            )
        });

        std::fs::create_dir_all(&dir)
            .map_err(|e| Error::Config(format!("create vm dir: {e}")))?;
        self.save_spec(&spec)?;

        let runtime = RuntimeState {
            state: VmState::Created,
            ..Default::default()
        };
        self.save_runtime(&req.name, &runtime)?;

        self.push_console(
            &req.name,
            format!(
                "[vsb] VM '{}' created ({cpus} vCPU, {mem} MB)",
                req.name,
                cpus = spec.cpus,
                mem = spec.memory_mb
            ),
        );

        self.get(&req.name)
    }

    /// 更新 VM 配置（运行中拒绝修改）。
    pub fn update(&self, name: &str, req: UpdateVmRequest) -> Result<VmSummary> {
        let mut spec = self.load_spec(name)?;
        let runtime = self.load_runtime(name);
        if matches!(runtime.state, VmState::Starting | VmState::Running) || self.is_boot_active(name)
        {
            return Err(Error::Config(format!(
                "VM '{name}' is running — stop it before editing"
            )));
        }
        if let Some(cpus) = req.cpus {
            if cpus == 0 {
                return Err(Error::Config("cpus must be >= 1".into()));
            }
            spec.cpus = cpus;
        }
        if let Some(mem) = req.memory_mb {
            if mem < 128 {
                return Err(Error::Config("memory_mb must be >= 128".into()));
            }
            spec.memory_mb = mem;
        }
        if let Some(disk) = req.disk_gb {
            spec.disk_gb = disk;
        }
        if let Some(cmdline) = req.cmdline {
            spec.cmdline = if cmdline.trim().is_empty() {
                None
            } else {
                Some(cmdline)
            };
        }
        self.save_spec(&spec)?;
        self.push_console(name, "[vsb] VM config updated".to_string());
        self.get(name)
    }

    /// 删除 VM 及其数据目录（运行中拒绝删除）。
    pub fn delete(&self, name: &str) -> Result<()> {
        let dir = self.vm_dir(name);
        if !dir.exists() {
            return Err(Error::Config(format!("VM '{name}' not found")));
        }
        if self.is_boot_active(name) {
            return Err(Error::Config(format!(
                "VM '{name}' is running — stop it before deleting"
            )));
        }
        std::fs::remove_dir_all(&dir).map_err(|e| Error::Config(format!("delete vm: {e}")))?;
        self.consoles.write().remove(name);
        Ok(())
    }

    /// 启动 VM。
    ///
    /// hypervisor + guest 镜像（assets/）就绪时在后台任务中执行真实 boot，
    /// 结束后把 `stopped` / `crashed` 写回 runtime state；否则退回演示模式。
    pub fn start(&self, name: &str) -> Result<VmSummary> {
        let spec = self.load_spec(name)?;
        let runtime = self.load_runtime(name);
        if matches!(runtime.state, VmState::Starting | VmState::Running) {
            return Err(Error::Config(format!("VM '{name}' is already running")));
        }

        let hv = crate::hypervisor::probe();
        self.push_console(name, format!("[vsb] hypervisor probe: {hv}"));

        let boot_opts = crate::vmm::boot::BootOptions::from_config(spec)?;
        let assets_ok = boot_opts.validate_assets().is_ok();
        let runtime_ok = tokio::runtime::Handle::try_current().is_ok();

        if hv.is_ok() && assets_ok && runtime_ok {
            if let Some(active) = self.active_boot.read().clone() {
                return Err(Error::Config(format!(
                    "another VM boot is in progress ('{active}') — only one real boot at a time"
                )));
            }
            self.spawn_boot(name, boot_opts)?;
        } else {
            // 演示模式：只推进状态机并说明缺了什么
            if !hv.is_ok() {
                self.push_console(
                    name,
                    "[vsb] hypervisor unavailable — build with --features whvp (Windows) / kvm (Linux)"
                        .to_string(),
                );
            } else if !assets_ok {
                self.push_console(
                    name,
                    "[vsb] guest images missing — run scripts/prepare-guest.sh + scripts/quick-rootfs.sh"
                        .to_string(),
                );
            }
            self.push_console(
                name,
                "[vsb] (demo) marked running — real boot requires hypervisor + assets/".to_string(),
            );
            self.set_state(name, VmState::Running)?;
        }

        self.get(name)
    }

    /// 在 tokio 后台任务中执行真实 boot 并回写最终状态。
    fn spawn_boot(&self, name: &str, opts: crate::vmm::boot::BootOptions) -> Result<()> {
        *self.active_boot.write() = Some(name.to_string());
        self.set_state(name, VmState::Starting)?;

        let store = self.clone();
        let name = name.to_string();
        tokio::spawn(async move {
            // [vsb] 管线日志 → 控制台缓冲
            let log: crate::vmm::boot::BootLog = {
                let store = store.clone();
                let name = name.clone();
                Arc::new(move |line: String| store.push_console(&name, line))
            };
            // [guest] 串口行 → 控制台缓冲（全局钩子，boot 期间独占）
            {
                let store = store.clone();
                let name = name.clone();
                crate::guest_log::set_hook(Some(Arc::new(move |line: &str| {
                    store.push_console(&name, format!("[guest] {line}"));
                })));
            }

            let _ = store.set_state(&name, VmState::Running);
            let result = crate::vmm::boot::run_boot(opts, log).await;
            crate::guest_log::set_hook(None);

            let final_state = match result {
                Ok(code) => {
                    store.push_console(&name, format!("[vsb] VM exited (code {code})"));
                    if code == 0 {
                        VmState::Stopped
                    } else {
                        VmState::Crashed
                    }
                }
                Err(e) => {
                    store.push_console(&name, format!("[vsb] boot failed: {e}"));
                    VmState::Crashed
                }
            };
            *store.active_boot.write() = None;
            if let Err(e) = store.set_state(&name, final_state) {
                tracing::warn!("write back VM state failed: {e}");
            }
        });
        Ok(())
    }

    /// 停止 VM（真实 boot 时请求 vCPU 优雅退出）。
    pub fn stop(&self, name: &str) -> Result<VmSummary> {
        if !self.vm_dir(name).exists() {
            return Err(Error::Config(format!("VM '{name}' not found")));
        }
        if self.is_boot_active(name) && crate::guest_control::request_stop() {
            // 真实 boot：请求 vCPU 优雅退出，boot 任务结束后回写 stopped/crashed
            self.push_console(name, "[vsb] stop requested — waiting for vCPU to exit".to_string());
            self.set_state(name, VmState::Stopping)?;
        } else {
            self.push_console(name, "[vsb] VM stopped".to_string());
            self.set_state(name, VmState::Stopped)?;
        }
        self.get(name)
    }

    /// 重启 VM（先停止再启动）。
    pub fn reboot(&self, name: &str) -> Result<VmSummary> {
        if self.is_boot_active(name) {
            // 真实 boot：先停，等 vCPU 退出后由后台任务重新启动
            self.stop(name)?;
            let store = self.clone();
            let name_owned = name.to_string();
            if tokio::runtime::Handle::try_current().is_ok() {
                tokio::spawn(async move {
                    for _ in 0..100 {
                        if !store.is_boot_active(&name_owned) {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                    store.push_console(&name_owned, "[vsb] rebooting…".to_string());
                    if let Err(e) = store.start(&name_owned) {
                        store.push_console(&name_owned, format!("[vsb] reboot failed: {e}"));
                    }
                });
            }
            return self.get(name);
        }
        self.push_console(name, "[vsb] rebooting…".to_string());
        self.stop(name)?;
        self.start(name)
    }

    /// 优雅关机（当前等同于 [`Self::stop`]）。
    pub fn shutdown(&self, name: &str) -> Result<VmSummary> {
        self.stop(name)
    }

    /// 写入运行时状态（Running 时记录 started_at，Stopped/Crashed 时清零并落盘控制台）。
    fn set_state(&self, name: &str, next: VmState) -> Result<()> {
        if !self.vm_dir(name).exists() {
            return Err(Error::Config(format!("VM '{name}' not found")));
        }
        let mut runtime = self.load_runtime(name);
        runtime.state = next;
        if next == VmState::Running {
            runtime.started_at = chrono::Utc::now().timestamp();
        }
        if matches!(next, VmState::Stopped | VmState::Crashed) {
            runtime.started_at = 0;
        }
        runtime.console_lines = self.consoles.read().get(name).cloned().unwrap_or_default();
        self.save_runtime(name, &runtime)
    }

    fn load_spec(&self, name: &str) -> Result<VmConfig> {
        let path = self.vm_dir(name).join(VM_MANIFEST);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| Error::Config(format!("read {path:?}: {e}")))?;
        serde_json::from_str(&text).map_err(|e| Error::Config(format!("parse vm.json: {e}")))
    }

    fn save_spec(&self, spec: &VmConfig) -> Result<()> {
        let path = self.vm_dir(&spec.name).join(VM_MANIFEST);
        let text = serde_json::to_string_pretty(spec)
            .map_err(|e| Error::Config(format!("serialize vm.json: {e}")))?;
        write_atomic(&path, &text)
    }

    fn load_runtime(&self, name: &str) -> RuntimeState {
        let path = self.vm_dir(name).join(RUNTIME_STATE);
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    fn save_runtime(&self, name: &str, state: &RuntimeState) -> Result<()> {
        let path = self.vm_dir(name).join(RUNTIME_STATE);
        let text = serde_json::to_string_pretty(state)
            .map_err(|e| Error::Config(format!("serialize runtime_state: {e}")))?;
        write_atomic(&path, &text)
    }
}

fn write_atomic(path: &Path, content: &str) -> Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, content).map_err(|e| Error::Config(format!("write {tmp:?}: {e}")))?;
    std::fs::rename(&tmp, path).map_err(|e| Error::Config(format!("rename vm file: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, VmStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = VmStore::new(dir.path()).unwrap();
        (dir, store)
    }

    fn create_req(name: &str) -> CreateVmRequest {
        CreateVmRequest {
            name: name.into(),
            cpus: 2,
            memory_mb: 1024,
            disk_gb: 5,
            cmdline: None,
            kernel: None,
            initramfs: None,
            rootfs: None,
        }
    }

    #[test]
    fn create_get_list_delete() {
        let (_dir, store) = store();
        let vm = store.create(create_req("alpha")).unwrap();
        assert_eq!(vm.state, VmState::Created);
        assert_eq!(vm.cpus, 2);
        assert!(!vm.console_lines.is_empty());

        assert_eq!(store.list().unwrap().len(), 1);
        store.delete("alpha").unwrap();
        assert!(store.get("alpha").is_err());
    }

    #[test]
    fn create_rejects_bad_names() {
        let (_dir, store) = store();
        assert!(store.create(create_req("")).is_err());
        assert!(store.create(create_req("has space")).is_err());
        assert!(store.create(create_req("has/slash")).is_err());
    }

    #[test]
    fn update_changes_spec_and_rejects_running() {
        let (_dir, store) = store();
        store.create(create_req("beta")).unwrap();
        let vm = store
            .update(
                "beta",
                UpdateVmRequest {
                    cpus: Some(4),
                    memory_mb: Some(4096),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(vm.cpus, 4);
        assert_eq!(vm.memory_mb, 4096);

        store.set_state("beta", VmState::Running).unwrap();
        assert!(store
            .update("beta", UpdateVmRequest::default())
            .is_err());
    }

    #[test]
    fn console_tail_is_incremental() {
        let (_dir, store) = store();
        store.create(create_req("gamma")).unwrap();
        let (lines, cursor) = store.console_tail("gamma", 0).unwrap();
        assert!(!lines.is_empty());
        let (rest, cursor2) = store.console_tail("gamma", cursor).unwrap();
        assert!(rest.is_empty());
        assert_eq!(cursor, cursor2);
        store.push_console("gamma", "[vsb] more");
        let (rest, _) = store.console_tail("gamma", cursor).unwrap();
        assert_eq!(rest, ["[vsb] more"]);
    }

    #[test]
    fn console_lines_do_not_duplicate_after_state_write() {
        let (_dir, store) = store();
        store.create(create_req("delta")).unwrap();
        let before = store.get("delta").unwrap().console_lines.len();
        store.set_state("delta", VmState::Stopped).unwrap();
        let after = store.get("delta").unwrap().console_lines.len();
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn start_without_hypervisor_falls_back_to_demo() {
        let (_dir, store) = store();
        store.create(create_req("demo")).unwrap();
        // 测试环境无 assets/（即便 KVM 可用也会走演示模式）
        let vm = store.start("demo").unwrap();
        assert_eq!(vm.state, VmState::Running);
        assert!(vm.started_at.is_some());

        let vm = store.stop("demo").unwrap();
        assert_eq!(vm.state, VmState::Stopped);
        assert!(vm.started_at.is_none());
    }
}
