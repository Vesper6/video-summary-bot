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
    pub available: bool,
    pub backend: &'static str,
    pub error: Option<&'static str>,
}

impl ProbeResult {
    pub fn ok(backend: &'static str) -> Self {
        Self {
            available: true,
            backend,
            error: None,
        }
    }

    pub fn err(backend: &'static str, error: &'static str) -> Self {
        Self {
            available: false,
            backend,
            error: Some(error),
        }
    }

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

    /// 后端名称。
    fn backend_name(&self) -> &'static str;
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
        Err(crate::error::Error::Vmm(
            "unsupported platform".to_string(),
        ))
    }
}