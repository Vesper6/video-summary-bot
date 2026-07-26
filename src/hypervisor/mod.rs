//! 跨平台 Hypervisor 抽象层。
//!
//! ## 设计原则
//!
//! 每个平台用**原生最优**的 hypervisor：
//! - Windows → WHVP（Windows Hypervisor Platform）
//! - macOS → Hypervisor Framework
//! - Linux → KVM
//!
//! 我们不试图抽象统一，而是提供统一的 trait，让平台实现各自的特点。

use std::sync::Arc;

use async_trait::async_trait;

use crate::error::Result;
use crate::vmm::VmConfig;

pub mod linux;
pub mod macos;
pub mod windows;

/// Hypervisor 探测结果。
#[derive(Debug, Clone, Copy)]
pub struct ProbeResult {
    /// 是否可用
    pub available: bool,
    /// 后端名称
    pub backend: &'static str,
    /// 不可用时的错误说明
    pub error: Option<&'static str>,
}

impl ProbeResult {
    /// 构造「可用」结果。
    pub fn ok(backend: &'static str) -> Self {
        Self {
            available: true,
            backend,
            error: None,
        }
    }

    /// 构造「不可用」结果。
    pub fn err(backend: &'static str, error: &'static str) -> Self {
        Self {
            available: false,
            backend,
            error: Some(error),
        }
    }

    /// 后端是否可用。
    pub fn is_ok(&self) -> bool {
        self.available
    }
}

impl std::fmt::Display for ProbeResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.available {
            write!(f, "✓ {} (可用)", self.backend)
        } else {
            write!(
                f,
                "✗ {} (不可用{})",
                self.backend,
                self.error.map(|e| format!(": {e}")).unwrap_or_default()
            )
        }
    }
}

/// vCPU run loop 的设备 I/O 分发钩子。
///
/// 由 [`crate::vmm::Vmm`] 实现：PIT / VirtIO MMIO 总线等平台无关设备模型
/// 通过它接入各 hypervisor 后端的 exit 处理。
pub trait VmIoHandler: Send + Sync {
    /// 端口读。返回 `None` 表示未处理（后端回退默认行为，如返回 0xFF）。
    fn pio_read(&self, port: u16, size: u8) -> Option<u32>;
    /// 端口写。返回 `false` 表示未处理。
    fn pio_write(&self, port: u16, value: u32, size: u8) -> bool;
    /// MMIO 读。返回 `None` 表示地址不属于任何设备。
    fn mmio_read(&self, gpa: u64, size: u8) -> Option<u64>;
    /// MMIO 写。返回 `false` 表示地址不属于任何设备。
    fn mmio_write(&self, gpa: u64, value: u64, size: u8) -> bool;
    /// 取走一个待注入的 IRQ 线号（run loop 每轮轮询，注入后边沿清除）。
    fn take_pending_irq(&self) -> Option<u8>;
}

/// Hypervisor 平台 trait。
#[async_trait]
pub trait Hypervisor: Send + Sync {
    /// 探测当前平台 hypervisor 可用性。
    fn probe() -> ProbeResult
    where
        Self: Sized;

    /// 创建 VM。
    async fn create_vm(&self, config: &VmConfig) -> Result<()>;

    /// 启动 VM（开始 vCPU 调度）。
    async fn start(&self) -> Result<()>;

    /// 停止 VM。
    async fn stop(&self) -> Result<()>;

    /// 阻塞运行直到 VM 退出，返回退出码。
    async fn run(&self) -> Result<i32>;

    /// 把宿主 RAM 映射到客户机物理地址空间。
    fn map_ram(&self, hva: *mut u8, gpa: u64, size: u64) -> Result<()>;

    /// 设置 vCPU 0 的完整 long-mode 入口状态。
    /// 必须在 `create_vm()` 之后、`run()` 之前调用。
    #[allow(clippy::too_many_arguments)]
    fn set_vcpu_entry(
        &self,
        rip: u64,
        rsp: u64,
        rsi: u64,
        cr0: u64,
        cr3: u64,
        cr4: u64,
        efer: u64,
        gdt_base: u64,
        gdt_limit: u16,
    ) -> Result<()>;

    /// 后端名称。
    fn backend_name(&self) -> &'static str;

    /// 请求停止 run 循环（可选，默认忽略）。
    /// run 循环会在下一轮检查此标志并优雅退出。
    fn request_stop(&self) {}

    /// 注册设备 I/O 分发钩子（run 之前调用；默认忽略）。
    fn set_io_handler(&self, _handler: Arc<dyn VmIoHandler>) {}
}

/// 探测当前平台 hypervisor。
pub fn probe() -> ProbeResult {
    #[cfg(target_os = "windows")]
    {
        windows::WhvpBackend::probe()
    }
    #[cfg(target_os = "macos")]
    {
        macos::HvfBackend::probe()
    }
    #[cfg(target_os = "linux")]
    {
        linux::KvmBackend::probe()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        ProbeResult::err("unknown", "unsupported platform")
    }
}

/// 获取后端名称。
pub fn backend_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "WHVP"
    }
    #[cfg(target_os = "macos")]
    {
        "Hypervisor Framework"
    }
    #[cfg(target_os = "linux")]
    {
        "KVM"
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        "unknown"
    }
}

/// 创建当前平台的 hypervisor 后端实例。
pub fn create() -> Result<Arc<dyn Hypervisor>> {
    #[cfg(target_os = "windows")]
    {
        Ok(Arc::new(windows::WhvpBackend::new()?))
    }
    #[cfg(target_os = "macos")]
    {
        Ok(Arc::new(macos::HvfBackend::new()?))
    }
    #[cfg(target_os = "linux")]
    {
        Ok(Arc::new(linux::KvmBackend::new()?))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        Err(crate::error::Error::Vmm("unsupported platform".to_string()))
    }
}
