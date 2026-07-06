//! VM 端到端启动（CLI `vm boot` 与 GUI API 共用）。

use std::path::PathBuf;
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::vmm::{VmConfig, Vmm};

/// 默认启动超时（秒）。
pub const DEFAULT_BOOT_TIMEOUT_SECS: u64 = 120;

/// 日志回调（`[vsb]` 与 `[guest]` 行）。
pub type BootLog = Arc<dyn Fn(String) + Send + Sync>;

/// 启动参数。
#[derive(Debug, Clone)]
pub struct BootOptions {
    pub config: VmConfig,
    pub kernel: PathBuf,
    pub initrd: PathBuf,
    pub rootfs: PathBuf,
    pub timeout_secs: u64,
}

impl BootOptions {
    /// 从 `VmConfig` 与默认/自定义镜像路径构建启动参数。
    pub fn from_config(config: VmConfig) -> Result<Self> {
        let kernel = config
            .kernel
            .clone()
            .unwrap_or_else(|| assets_dir().join("kernels/vmlinuz"));
        let initrd = config
            .initramfs
            .clone()
            .unwrap_or_else(|| assets_dir().join("initramfs/initrd.img"));
        let rootfs = config
            .rootfs
            .clone()
            .unwrap_or_else(|| assets_dir().join("rootfs/rootfs.img"));

        Ok(Self {
            config,
            kernel,
            initrd,
            rootfs,
            timeout_secs: DEFAULT_BOOT_TIMEOUT_SECS,
        })
    }

    /// 校验内核 / initrd / rootfs 是否存在。
    pub fn validate_assets(&self) -> Result<()> {
        for (label, path) in [
            ("kernel", &self.kernel),
            ("initrd", &self.initrd),
            ("rootfs", &self.rootfs),
        ] {
            if !path.exists() {
                return Err(Error::Vmm(format!(
                    "{label} not found: {} (run scripts/prepare-guest.sh)",
                    path.display()
                )));
            }
        }
        Ok(())
    }
}

/// 解析 `assets/` 目录（开发树 / 安装目录均可）。
pub fn assets_dir() -> PathBuf {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("assets"));
        }
    }
    candidates.push(PathBuf::from("./assets"));
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"));
    for c in candidates {
        if c.exists() {
            return c;
        }
    }
    PathBuf::from("./assets")
}

fn log_line(log: &BootLog, line: impl Into<String>) {
    log(line.into());
}

/// 端到端启动：加载内核 → 映射 RAM → 跑 vCPU。
pub async fn run_boot(opts: BootOptions, log: BootLog) -> Result<i32> {
    opts.validate_assets()?;

    let mut config = opts.config;
    config.kernel = Some(opts.kernel.clone());
    config.initramfs = Some(opts.initrd.clone());
    config.rootfs = Some(opts.rootfs.clone());

    log_line(&log, format!("[vsb] booting VM '{}' …", config.name));
    log_line(
        &log,
        format!(
            "[vsb] {} vCPU, {} MB RAM",
            config.cpus, config.memory_mb
        ),
    );
    log_line(&log, format!("[vsb] kernel: {}", opts.kernel.display()));
    log_line(&log, format!("[vsb] initrd: {}", opts.initrd.display()));
    log_line(&log, format!("[vsb] rootfs: {}", opts.rootfs.display()));

    let vmm = Vmm::new(config)?;
    log_line(&log, "[vsb] VMM created, loading kernel…");

    let regs = vmm.load_kernel()?;
    log_line(
        &log,
        format!(
            "[vsb] kernel @ {:#x}, boot_params @ {:#x}",
            regs.rip, regs.rsi
        ),
    );

    let hv_status = crate::hypervisor::probe();
    log_line(&log, format!("[vsb] hypervisor: {hv_status}"));

    if !hv_status.is_ok() {
        log_line(
            &log,
            "[vsb] hypervisor unavailable — rebuild with --features whvp (Windows) or kvm (Linux)",
        );
        return Err(Error::Vmm("hypervisor not available".into()));
    }

    let hv = crate::hypervisor::create()?;
    hv.create_vm(&vmm.config).await?;

    // 注册停止钩子：API/GUI 的 stop 会通过 guest_control::request_stop() 中断 vCPU
    {
        let hv_for_stop = Arc::clone(&hv);
        crate::guest_control::set_hook(Some(Arc::new(move || {
            hv_for_stop.request_stop();
        })));
    }

    let ram_ptr = vmm.ram.raw_ptr();
    let ram_size = vmm.ram.size() as u64;
    log_line(
        &log,
        format!("[vsb] mapping RAM {ram_size} bytes @ GPA 0x0…"),
    );

    match hv.map_ram(ram_ptr, 0, ram_size) {
        Ok(()) => log_line(&log, "[vsb] RAM mapped OK"),
        Err(e) => log_line(&log, format!("[vsb] RAM map failed: {e}")),
    }

    let gdt_base = crate::vmm::loader::GDT_ADDR;
    let gdt_limit = (crate::vmm::loader::GDT_ENTRIES * 8 - 1) as u16;
    hv.set_vcpu_entry(
        regs.rip,
        regs.rsp,
        regs.rsi,
        regs.cr0,
        regs.cr3,
        regs.cr4,
        regs.efer,
        gdt_base,
        gdt_limit,
    )?;
    log_line(
        &log,
        format!(
            "[vsb] vCPU entry RIP={:#x} CR3={:#x}",
            regs.rip, regs.cr3
        ),
    );

    hv.start().await?;
    log_line(&log, "[vsb] vCPU running — guest serial on COM1 (ttyS0)…");

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(opts.timeout_secs),
        hv.run(),
    )
    .await;

    let exit_code = match result {
        Ok(Ok(code)) => {
            log_line(&log, format!("[vsb] vCPU exited with code {code}"));
            code
        }
        Ok(Err(e)) => {
            log_line(&log, format!("[vsb] vCPU error: {e}"));
            crate::guest_control::set_hook(None);
            return Err(e);
        }
        Err(_) => {
            log_line(
                &log,
                format!("[vsb] timeout after {}s, stopping", opts.timeout_secs),
            );
            hv.stop().await?;
            0
        }
    };

    // 清除停止钩子（vCPU 已退出）
    crate::guest_control::set_hook(None);

    Ok(exit_code)
}