//! VM 配置持久化（参考 tenbox `vm.json` + `runtime_state.json`）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::vmm::{VmConfig, VmState};

const VM_MANIFEST: &str = "vm.json";
const RUNTIME_STATE: &str = "runtime_state.json";

/// API 返回的 VM 摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmSummary {
    pub name: String,
    pub state: VmState,
    pub cpus: u8,
    pub memory_mb: u32,
    pub disk_gb: u32,
    pub cmdline: Option<String>,
    pub started_at: Option<i64>,
    pub console_lines: Vec<String>,
}

/// 创建 VM 请求。
#[derive(Debug, Deserialize)]
pub struct CreateVmRequest {
    pub name: String,
    #[serde(default = "default_cpus")]
    pub cpus: u8,
    #[serde(default = "default_memory")]
    pub memory_mb: u32,
    #[serde(default = "default_disk")]
    pub disk_gb: u32,
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

pub struct VmStore {
    root: PathBuf,
    consoles: Arc<RwLock<ConsoleMap>>,
}

impl VmStore {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        std::fs::create_dir_all(&root)
            .map_err(|e| Error::Config(format!("create vm store: {e}")))?;
        Ok(Self {
            root,
            consoles: Arc::new(RwLock::new(HashMap::new())),
        })
    }

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
        if buf.len() > 500 {
            let drain = buf.len() - 500;
            buf.drain(0..drain);
        }
    }

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

    pub fn get(&self, name: &str) -> Result<VmSummary> {
        let spec = self.load_spec(name)?;
        let runtime = self.load_runtime(name);
        let console = self.consoles.read().get(name).cloned().unwrap_or_default();
        let mut lines = runtime.console_lines;
        lines.extend(console);
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
            console_lines: lines,
        })
    }

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
            format!("[vsb] VM '{}' created ({cpus} vCPU, {mem} MB)", req.name, cpus = spec.cpus, mem = spec.memory_mb),
        );

        self.get(&req.name)
    }

    pub fn delete(&self, name: &str) -> Result<()> {
        let dir = self.vm_dir(name);
        if !dir.exists() {
            return Err(Error::Config(format!("VM '{}' not found", name)));
        }
        std::fs::remove_dir_all(&dir)
            .map_err(|e| Error::Config(format!("delete vm: {e}")))?;
        self.consoles.write().remove(name);
        Ok(())
    }

    pub fn start(&self, name: &str) -> Result<VmSummary> {
        self.transition(name, VmState::Starting, |store, name| {
            let hv = crate::hypervisor::probe();
            store.push_console(name, format!("[vsb] hypervisor probe: {hv}"));
            if hv.is_ok() {
                store.push_console(
                    name,
                    "[vsb] VMM boot pipeline ready — use `vsb vm boot` for full WHVP run".to_string(),
                );
                store.push_console(
                    name,
                    "[guest] (demo) waiting for virtio console / kernel banner…".to_string(),
                );
            } else {
                store.push_console(
                    name,
                    "[vsb] hypervisor unavailable — build with --features whvp (Windows)".to_string(),
                );
            }
            VmState::Running
        })
    }

    pub fn stop(&self, name: &str) -> Result<VmSummary> {
        self.transition(name, VmState::Stopping, |store, name| {
            store.push_console(name, "[vsb] VM stopped".to_string());
            VmState::Stopped
        })
    }

    pub fn reboot(&self, name: &str) -> Result<VmSummary> {
        self.transition(name, VmState::Stopping, |store, name| {
            store.push_console(name, "[vsb] rebooting…".to_string());
            VmState::Starting
        })?;
        self.transition(name, VmState::Starting, |store, name| {
            store.push_console(name, "[vsb] VM running after reboot".to_string());
            VmState::Running
        })
    }

    pub fn shutdown(&self, name: &str) -> Result<VmSummary> {
        self.stop(name)
    }

    fn transition<F>(&self, name: &str, _phase: VmState, apply: F) -> Result<VmSummary>
    where
        F: FnOnce(&Self, &str) -> VmState,
    {
        if !self.vm_dir(name).exists() {
            return Err(Error::Config(format!("VM '{}' not found", name)));
        }
        let mut runtime = self.load_runtime(name);
        let next = apply(self, name);
        runtime.state = next;
        if next == VmState::Running {
            runtime.started_at = chrono::Utc::now().timestamp();
        }
        if next == VmState::Stopped {
            runtime.started_at = 0;
        }
        let console = self.consoles.read().get(name).cloned().unwrap_or_default();
        runtime.console_lines = console;
        self.save_runtime(name, &runtime)?;
        self.get(name)
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