//! macOS 后端：Hypervisor Framework
//!
//! Apple 提供的 hypervisor 框架：
//! - `hv_vm_create` — 创建 VM
//! - `hv_vm_map` — 映射客户机物理内存
//! - `hv_vcpu_create` — 创建 vCPU
//! - `hv_vcpu_run` — 运行 vCPU

use async_trait::async_trait;

use crate::error::{Error, Result};
use crate::hypervisor::{Hypervisor, ProbeResult};
use crate::vmm::VmConfig;

/// macOS Hypervisor Framework 后端（占位实现）。
pub struct HvfBackend {
    // hv_vm_t（实际是 *mut c_void）
    vm_handle: Option<usize>,
}

impl HvfBackend {
    pub fn new() -> Result<Self> {
        Ok(Self { vm_handle: None })
    }
}

#[async_trait]
impl Hypervisor for HvfBackend {
    fn probe() -> ProbeResult {
        #[cfg(target_os = "macos")]
        {
            // 真实实现：调用 hv_vm_create 探测
            ProbeResult::ok("Hypervisor Framework")
        }
        #[cfg(not(target_os = "macos"))]
        {
            ProbeResult::err("Hypervisor Framework", "not running on macOS")
        }
    }

    fn backend_name(&self) -> &'static str {
        "Hypervisor Framework"
    }

    async fn create_vm(&self, _config: &VmConfig) -> Result<()> {
        Err(Error::Vmm("HVF create_vm not implemented".into()))
    }

    async fn start(&self) -> Result<()> {
        Err(Error::Vmm("HVF start not implemented".into()))
    }

    async fn stop(&self) -> Result<()> {
        Err(Error::Vmm("HVF stop not implemented".into()))
    }

    async fn run(&self) -> Result<i32> {
        Err(Error::Vmm("HVF run not implemented".into()))
    }
}