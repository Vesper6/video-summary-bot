//! Windows 后端：WHVP（Windows Hypervisor Platform）
//!
//! WHVP API 通过 `Win32_System_Hypervisor` 暴露。
//! 主要函数：
//! - `WHvCreatePartition` — 创建 VM 分区
//! - `WHvMapGpaRange` — 映射客户机物理地址到宿主
//! - `WHvRunPartition` — 运行 vCPU
//! - `WHvDeletePartition` — 销毁分区

use async_trait::async_trait;

use crate::error::{Error, Result};
use crate::hypervisor::{Hypervisor, ProbeResult};
use crate::vmm::VmConfig;

/// Windows WHVP 后端（占位实现）。
pub struct WhvpBackend {
    partition_handle: Option<usize>, // WHV_PARTITION_HANDLE 实际是 *mut c_void
}

impl WhvpBackend {
    pub fn new() -> Result<Self> {
        // 真实实现：
        // 1. 加载 `Win32_System_Hypervisor` 函数
        // 2. 调用 WHvCreatePartition
        // 3. 配置 vCPU 数量、客户机内存、特性等
        // 4. 返回分区句柄
        Ok(Self {
            partition_handle: None,
        })
    }
}

#[async_trait]
impl Hypervisor for WhvpBackend {
    fn probe() -> ProbeResult {
        // 真实实现：检查 Windows 版本、Hyper-V 功能是否启用
        // - Windows 10/11 Pro/Enterprise/Education：默认启用
        // - 检查 HCS 合约（Host Compute Service）可用性
        #[cfg(target_os = "windows")]
        {
            ProbeResult::ok("WHVP")
        }
        #[cfg(not(target_os = "windows"))]
        {
            ProbeResult::err("WHVP", "not running on Windows")
        }
    }

    fn backend_name(&self) -> &'static str {
        "WHVP"
    }

    async fn create_vm(&self, _config: &VmConfig) -> Result<()> {
        // TODO: 调用 WHvCreatePartition + WHvSetupPartition
        Err(Error::Vmm("WHVP create_vm not implemented".into()))
    }

    async fn start(&self) -> Result<()> {
        Err(Error::Vmm("WHVP start not implemented".into()))
    }

    async fn stop(&self) -> Result<()> {
        Err(Error::Vmm("WHVP stop not implemented".into()))
    }

    async fn run(&self) -> Result<i32> {
        Err(Error::Vmm("WHVP run not implemented".into()))
    }
}

// 仅在 Windows 上编译时引入 windows crate
#[cfg(target_os = "windows")]
mod windows_bindings {
    // 这里将来绑定 Win32_System_Hypervisor
    // use windows::Win32::System::Hypervisor::*;
}