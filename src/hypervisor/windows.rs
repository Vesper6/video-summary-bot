//! Windows 后端：WHVP（Windows Hypervisor Platform）
//!
//! windows crate 0.58 API:
//! - `WHvCreatePartition() -> Result<WHV_PARTITION_HANDLE>`
//! - `WHvSetPartitionProperty(handle, code, ptr, size) -> Result<()>`
//! - `WHvMapGpaRange(handle, hva, gpa, size, flags) -> Result<()>`
//! - `WHvRunVirtualProcessor(handle, idx, ctx, size) -> Result<()>`

use async_trait::async_trait;
use std::sync::Arc;
use parking_lot::RwLock;

use crate::error::{Error, Result};
use crate::hypervisor::{Hypervisor, ProbeResult};
use crate::vmm::{VmConfig, VmState};

// =============================================
// Windows WHVP 实现（feature-gated）
// =============================================

#[cfg(all(target_os = "windows", feature = "whvp"))]
mod whvp_impl {
    use super::*;
    use windows::Win32::System::Hypervisor::*;

    /// WHVP Partition 包装（RAII）
    pub(super) struct WhvpPartition {
        handle: WHV_PARTITION_HANDLE,
        vcpu_count: u32,
    }

    impl WhvpPartition {
        pub fn create(vcpu_count: u32) -> Result<Self> {
            unsafe {
                let handle = WHvCreatePartition()
                    .map_err(|e| Error::Vmm(format!("WHvCreatePartition: {e}")))?;

                let property_code = WHvPartitionPropertyCodeProcessorCount;
                let property_value: u32 = vcpu_count;
                WHvSetPartitionProperty(
                    handle,
                    property_code,
                    &property_value as *const u32 as *const core::ffi::c_void,
                    std::mem::size_of::<u32>() as u32,
                )
                .map_err(|e| {
                    let _ = WHvDeletePartition(handle);
                    Error::Vmm(format!("WHvSetPartitionProperty: {e}"))
                })?;

                WHvSetupPartition(handle).map_err(|e| {
                    let _ = WHvDeletePartition(handle);
                    Error::Vmm(format!("WHvSetupPartition: {e}"))
                })?;

                tracing::info!("WHVP partition created (cpus={})", vcpu_count);
                Ok(Self { handle, vcpu_count })
            }
        }

        pub fn vcpu_count(&self) -> u32 {
            self.vcpu_count
        }

        pub fn map_gpa_range(
            &self,
            host_ptr: *mut core::ffi::c_void,
            guest_address: u64,
            size: u64,
            flags: WHV_MAP_GPA_RANGE_FLAGS,
        ) -> Result<()> {
            unsafe {
                WHvMapGpaRange(self.handle, host_ptr, guest_address, size, flags)
                    .map_err(|e| Error::Vmm(format!(
                        "WHvMapGpaRange (gpa={:#x}): {e}", guest_address
                    )))?;
                Ok(())
            }
        }

        pub fn create_vcpu(&self, vp_index: u32) -> Result<()> {
            unsafe {
                WHvCreateVirtualProcessor(self.handle, vp_index, 0)
                    .map_err(|e| Error::Vmm(format!(
                        "WHvCreateVirtualProcessor({}): {e}", vp_index
                    )))?;
                Ok(())
            }
        }

        pub fn delete_vcpu(&self, vp_index: u32) {
            unsafe { let _ = WHvDeleteVirtualProcessor(self.handle, vp_index); }
        }

        pub fn set_initial_registers(&self, vp_index: u32, entry_rip: u64) -> Result<()> {
            unsafe {
                let reg_names: [WHV_REGISTER_NAME; 3] = [
                    WHvX64RegisterRip,
                    WHvX64RegisterRflags,
                    WHvX64RegisterCr0,
                ];
                let reg_values = [
                    WHV_REGISTER_VALUE { Reg64: entry_rip },
                    WHV_REGISTER_VALUE { Reg64: 0x0000_0002 },
                    WHV_REGISTER_VALUE { Reg64: 0x0000_0010 },
                ];
                WHvSetVirtualProcessorRegisters(
                    self.handle,
                    vp_index,
                    reg_names.as_ptr(),
                    reg_names.len() as u32,
                    reg_values.as_ptr(),
                )
                .map_err(|e| Error::Vmm(format!("WHvSetVirtualProcessorRegisters: {e}")))?;
                Ok(())
            }
        }

        pub fn run_vcpu(&self, vp_index: u32) -> Result<WHV_RUN_VP_EXIT_CONTEXT> {
            unsafe {
                let mut exit_ctx: WHV_RUN_VP_EXIT_CONTEXT = std::mem::zeroed();
                WHvRunVirtualProcessor(
                    self.handle,
                    vp_index,
                    &mut exit_ctx as *mut _ as *mut core::ffi::c_void,
                    std::mem::size_of::<WHV_RUN_VP_EXIT_CONTEXT>() as u32,
                )
                .map_err(|e| Error::Vmm(format!("WHvRunVirtualProcessor({}): {e}", vp_index)))?;
                Ok(exit_ctx)
            }
        }
    }

    impl Drop for WhvpPartition {
        fn drop(&mut self) {
            unsafe {
                if !self.handle.is_invalid() {
                    let _ = WHvDeletePartition(self.handle);
                }
            }
        }
    }
}

// =============================================
// 公开 API
// =============================================

#[cfg(all(target_os = "windows", feature = "whvp"))]
struct WhvpState {
    partition: Option<whvp_impl::WhvpPartition>,
    vm_state: VmState,
}

pub struct WhvpBackend {
    #[cfg(all(target_os = "windows", feature = "whvp"))]
    state: Arc<RwLock<WhvpState>>,
}

impl WhvpBackend {
    pub fn new() -> Result<Self> {
        #[cfg(all(target_os = "windows", feature = "whvp"))]
        {
            Ok(Self {
                state: Arc::new(RwLock::new(WhvpState {
                    partition: None,
                    vm_state: VmState::Created,
                })),
            })
        }
        #[cfg(not(all(target_os = "windows", feature = "whvp")))]
        {
            Err(Error::Vmm(
                "WHVP backend requires Windows + 'whvp' feature. \
                 Build with: cargo build --features whvp".into(),
            ))
        }
    }
}

#[async_trait]
impl Hypervisor for WhvpBackend {
    fn probe() -> ProbeResult {
        #[cfg(all(target_os = "windows", feature = "whvp"))]
        {
            unsafe {
                use windows::Win32::System::Hypervisor::{
                    WHvCapabilityCodeHypervisorPresent, WHvGetCapability,
                };
                let mut capability: u32 = 0;
                let mut written_size: u32 = 0;
                let hr = WHvGetCapability(
                    WHvCapabilityCodeHypervisorPresent,
                    &mut capability as *mut u32 as *mut core::ffi::c_void,
                    std::mem::size_of::<u32>() as u32,
                    Some(&mut written_size),
                );
                if hr.is_ok() && capability != 0 {
                    ProbeResult::ok("WHVP")
                } else {
                    ProbeResult::err("WHVP", "hypervisor not present or not enabled")
                }
            }
        }

        #[cfg(not(all(target_os = "windows", feature = "whvp")))]
        {
            #[cfg(target_os = "windows")]
            {
                ProbeResult::err("WHVP", "feature 'whvp' not enabled - build with --features whvp")
            }
            #[cfg(not(target_os = "windows"))]
            {
                ProbeResult::err("WHVP", "not running on Windows")
            }
        }
    }

    fn backend_name(&self) -> &'static str {
        "WHVP"
    }

    async fn create_vm(&self, config: &VmConfig) -> Result<()> {
        #[cfg(all(target_os = "windows", feature = "whvp"))]
        {
            let mut state = self.state.write();
            if state.partition.is_some() {
                return Err(Error::Vmm("VM already created".into()));
            }
            tracing::info!(
                "Creating WHVP VM: {} (cpus={}, memory={}MB)",
                config.name, config.cpus, config.memory_mb
            );
            let partition = whvp_impl::WhvpPartition::create(config.cpus as u32)?;
            state.partition = Some(partition);
            state.vm_state = VmState::Created;
            Ok(())
        }

        #[cfg(not(all(target_os = "windows", feature = "whvp")))]
        Err(Error::Vmm("WHVP not available".into()))
    }

    async fn start(&self) -> Result<()> {
        #[cfg(all(target_os = "windows", feature = "whvp"))]
        {
            let mut state = self.state.write();
            if state.partition.is_none() {
                return Err(Error::Vmm("VM not created".into()));
            }
            state.vm_state = VmState::Running;
            Ok(())
        }

        #[cfg(not(all(target_os = "windows", feature = "whvp")))]
        Err(Error::Vmm("WHVP not available".into()))
    }

    async fn stop(&self) -> Result<()> {
        #[cfg(all(target_os = "windows", feature = "whvp"))]
        {
            let mut state = self.state.write();
            state.vm_state = VmState::Stopped;
            Ok(())
        }

        #[cfg(not(all(target_os = "windows", feature = "whvp")))]
        Err(Error::Vmm("WHVP not available".into()))
    }

    async fn run(&self) -> Result<i32> {
        #[cfg(all(target_os = "windows", feature = "whvp"))]
        {
            let state_arc = Arc::clone(&self.state);
            let result = tokio::task::spawn_blocking(move || -> Result<i32> {
                let state = state_arc.read();
                let partition = state.partition.as_ref()
                    .ok_or_else(|| Error::Vmm("VM not created".into()))?;
                let vcpu_count = partition.vcpu_count();
                tracing::info!("WHVP run loop starting, {} vCPUs", vcpu_count);

                for vp in 0..vcpu_count {
                    partition.create_vcpu(vp)?;
                    partition.set_initial_registers(vp, 0xFFF0)?;
                }

                let mut exit_code: i32 = 0;
                for _ in 0..1000 {
                    let exit_ctx = partition.run_vcpu(0)?;
                    match exit_ctx.ExitReason.0 {
                        8 => { tracing::info!("vCPU 0 halted"); break; }
                        other => {
                            tracing::debug!("WHVP exit reason: {}", other);
                            if other > 100 { break; }
                        }
                    }
                }

                for vp in 0..vcpu_count {
                    partition.delete_vcpu(vp);
                }

                Ok(exit_code)
            })
            .await
            .map_err(|e| Error::Vmm(format!("vCPU thread panicked: {e}")))?;

            self.state.write().vm_state = VmState::Stopped;
            result
        }

        #[cfg(not(all(target_os = "windows", feature = "whvp")))]
        Err(Error::Vmm("WHVP not available".into()))
    }
}