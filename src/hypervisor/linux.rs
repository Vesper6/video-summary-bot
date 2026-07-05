//! Linux 后端：KVM（Kernel-based Virtual Machine）
//!
//! KVM 通过 `/dev/kvm` 设备暴露：
//! - `KVM_CREATE_VM` — 创建 VM
//! - `KVM_CREATE_VCPU` — 创建 vCPU
//! - `KVM_RUN` — 运行 vCPU
//! - `KVM_SET_USER_MEMORY_REGION` — 设置客户机内存
//!
//! Rust crate：`kvm-ioctls` + `kvm-bindings`

use async_trait::async_trait;

use crate::error::{Error, Result};
use crate::hypervisor::{Hypervisor, ProbeResult};
use crate::vmm::VmConfig;

/// Linux KVM 后端（占位实现）。
pub struct KvmBackend {
    vm_fd: Option<i32>,
}

impl KvmBackend {
    pub fn new() -> Result<Self> {
        Ok(Self { vm_fd: None })
    }
}

#[async_trait]
impl Hypervisor for KvmBackend {
    fn probe() -> ProbeResult {
        #[cfg(target_os = "linux")]
        {
            // 真实实现：
            // 1. 检查 /dev/kvm 是否存在且可读写
            // 2. 尝试 open + KVM_GET_API_VERSION
            // 3. 验证 KVM_CAP_XXX 支持
            match std::fs::metadata("/dev/kvm") {
                Ok(_) => ProbeResult::ok("KVM"),
                Err(e) => ProbeResult::err("KVM", &format!("/dev/kvm: {e}")),
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            ProbeResult::err("KVM", "not running on Linux")
        }
    }

    fn backend_name(&self) -> &'static str {
        "KVM"
    }

    async fn create_vm(&self, _config: &VmConfig) -> Result<()> {
        // TODO: 使用 kvm-ioctls crate
        // let kvm = Kvm::new()?;
        // let vm = kvm.create_vm()?;
        Err(Error::Vmm("KVM create_vm not implemented".into()))
    }

    async fn start(&self) -> Result<()> {
        Err(Error::Vmm("KVM start not implemented".into()))
    }

    async fn stop(&self) -> Result<()> {
        Err(Error::Vmm("KVM stop not implemented".into()))
    }

    async fn run(&self) -> Result<i32> {
        Err(Error::Vmm("KVM run not implemented".into()))
    }
}