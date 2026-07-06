//! Windows 后端：WHVP（Windows Hypervisor Platform）
//!
//! WHVP API 通过 `Win32_System_Hypervisor` 暴露。
//! 主要函数：
//! - `WHvCreatePartition` — 创建 VM 分区
//! - `WHvMapGpaRange` — 映射客户机物理地址到宿主
//! - `WHvRunVirtualProcessor` — 运行 vCPU
//! - `WHvDeletePartition` — 销毁分区

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
    use std::ptr;
    use windows::Win32::System::Hypervisor::*;
    use windows::Win32::Foundation::*;

    /// WHVP Partition 包装（RAII 管理生命周期）
    pub(super) struct WhvpPartition {
        handle: WHV_PARTITION_HANDLE,
        vcpu_count: u32,
    }

    impl WhvpPartition {
        /// 创建新的 WHVP partition
        pub fn create(vcpu_count: u32) -> Result<Self> {
            unsafe {
                let mut handle = WHV_PARTITION_HANDLE::default();

                // 创建 partition
                let hr = WHvCreatePartition(&mut handle as *mut _);
                if hr.is_err() {
                    return Err(Error::Vmm(format!("WHvCreatePartition failed: {:?}", hr)));
                }

                // 设置 vCPU 数量
                let property_code = WHV_PARTITION_PROPERTY_CODE(0); // WHvPartitionPropertyCodeProcessorCount
                let property_value = vcpu_count;
                let hr = WHvSetPartitionProperty(
                    handle,
                    property_code,
                    &property_value as *const u32 as *const _,
                    std::mem::size_of::<u32>() as u32,
                );
                if hr.is_err() {
                    WHvDeletePartition(handle);
                    return Err(Error::Vmm(format!("WHvSetPartitionProperty failed: {:?}", hr)));
                }

                // Setup partition
                let hr = WHvSetupPartition(handle);
                if hr.is_err() {
                    WHvDeletePartition(handle);
                    return Err(Error::Vmm(format!("WHvSetupPartition failed: {:?}", hr)));
                }

                tracing::info!("WHVP partition created with {} vCPUs", vcpu_count);
                Ok(Self { handle, vcpu_count })
            }
        }

        pub fn handle(&self) -> WHV_PARTITION_HANDLE {
            self.handle
        }

        pub fn vcpu_count(&self) -> u32 {
            self.vcpu_count
        }

        /// 把宿主虚拟内存映射到客户机物理地址（GPA）。
        ///
        /// `host_ptr` 必须是页对齐的分配（4 KB）。
        /// `flags`：0 = 读写，1 = 只读，3 = 只执行等。
        pub fn map_gpa_range(
            &self,
            host_ptr: *mut std::ffi::c_void,
            guest_address: u64,
            size: u64,
            flags: WHV_MAP_GPA_RANGE_FLAGS,
        ) -> Result<()> {
            unsafe {
                let hr = WHvMapGpaRange(
                    self.handle,
                    host_ptr,
                    guest_address,
                    size,
                    flags,
                );
                if hr.is_err() {
                    return Err(Error::Vmm(format!(
                        "WHvMapGpaRange failed (gpa={:#x}, size={:#x}): {:?}",
                        guest_address, size, hr
                    )));
                }
                tracing::debug!(
                    "Mapped GPA {:#x}..{:#x} ({}KB)",
                    guest_address,
                    guest_address + size,
                    size / 1024
                );
                Ok(())
            }
        }

        /// 取消映射客户机物理地址范围。
        pub fn unmap_gpa_range(&self, guest_address: u64, size: u64) -> Result<()> {
            unsafe {
                let hr = WHvUnmapGpaRange(self.handle, guest_address, size);
                if hr.is_err() {
                    return Err(Error::Vmm(format!(
                        "WHvUnmapGpaRange failed (gpa={:#x}): {:?}",
                        guest_address, hr
                    )));
                }
                Ok(())
            }
        }

        /// 创建 vCPU。
        pub fn create_vcpu(&self, vp_index: u32) -> Result<()> {
            unsafe {
                let hr = WHvCreateVirtualProcessor(self.handle, vp_index, 0);
                if hr.is_err() {
                    return Err(Error::Vmm(format!(
                        "WHvCreateVirtualProcessor({}) failed: {:?}",
                        vp_index, hr
                    )));
                }
                tracing::debug!("vCPU {} created", vp_index);
                Ok(())
            }
        }

        /// 删除 vCPU。
        pub fn delete_vcpu(&self, vp_index: u32) {
            unsafe {
                let _ = WHvDeleteVirtualProcessor(self.handle, vp_index);
            }
        }

        /// 设置 vCPU 初始寄存器（实模式入口，CS:IP = 0xF000:0xFFF0）。
        pub fn set_initial_registers(&self, vp_index: u32, entry_rip: u64) -> Result<()> {
            unsafe {
                let reg_names = [
                    WHV_REGISTER_NAME(0x00000040), // WHvX64RegisterRip
                    WHV_REGISTER_NAME(0x00000041), // WHvX64RegisterRflags
                    WHV_REGISTER_NAME(0x00000044), // WHvX64RegisterCr0
                ];
                let reg_values = [
                    WHV_REGISTER_VALUE { Reg64: entry_rip },
                    WHV_REGISTER_VALUE { Reg64: 0x0000_0002 }, // RFLAGS: reserved bit set
                    WHV_REGISTER_VALUE { Reg64: 0x0000_0010 }, // CR0: PE=0 (real mode)
                ];

                let hr = WHvSetVirtualProcessorRegisters(
                    self.handle,
                    vp_index,
                    reg_names.as_ptr(),
                    reg_names.len() as u32,
                    reg_values.as_ptr(),
                );
                if hr.is_err() {
                    return Err(Error::Vmm(format!(
                        "WHvSetVirtualProcessorRegisters failed: {:?}", hr
                    )));
                }
                tracing::debug!("vCPU {} initial RIP = {:#x}", vp_index, entry_rip);
                Ok(())
            }
        }

        /// 运行 vCPU 直到发生 exit，返回 exit context。
        pub fn run_vcpu(&self, vp_index: u32) -> Result<WHV_RUN_VP_EXIT_CONTEXT> {
            unsafe {
                let mut exit_ctx: WHV_RUN_VP_EXIT_CONTEXT = std::mem::zeroed();
                let hr = WHvRunVirtualProcessor(
                    self.handle,
                    vp_index,
                    &mut exit_ctx as *mut WHV_RUN_VP_EXIT_CONTEXT as *mut _,
                    std::mem::size_of::<WHV_RUN_VP_EXIT_CONTEXT>() as u32,
                );
                if hr.is_err() {
                    return Err(Error::Vmm(format!(
                        "WHvRunVirtualProcessor({}) failed: {:?}", vp_index, hr
                    )));
                }
                Ok(exit_ctx)
            }
        }
    }

    /// 处理 vCPU 退出原因。
    pub(super) fn handle_exit(exit_ctx: &WHV_RUN_VP_EXIT_CONTEXT) -> VcpuAction {
        use crate::vmm::vcpu::VcpuAction;

        match exit_ctx.ExitReason {
            // WHvRunVpExitReasonX64IoPortAccess = 3
            WHV_RUN_VP_EXIT_REASON(3) => {
                let io = unsafe { &exit_ctx.Anonymous.IoPortAccess };
                tracing::trace!(
                    "I/O port access: port={:#x} write={} size={}",
                    io.PortNumber,
                    io.AccessInfo.IsWrite().0,
                    io.AccessInfo.AccessSize(),
                );
                VcpuAction::Continue
            }
            // WHvRunVpExitReasonMemoryAccess = 4
            WHV_RUN_VP_EXIT_REASON(4) => {
                let mmio = unsafe { &exit_ctx.Anonymous.MemoryAccess };
                tracing::trace!(
                    "MMIO access: gpa={:#x} write={}",
                    mmio.Gpa,
                    mmio.AccessInfo.IsWrite().0,
                );
                VcpuAction::Continue
            }
            // WHvRunVpExitReasonX64Halt = 8
            WHV_RUN_VP_EXIT_REASON(8) => {
                tracing::debug!("vCPU halted");
                VcpuAction::Stop
            }
            // WHvRunVpExitReasonX64Cpuid = 5
            WHV_RUN_VP_EXIT_REASON(5) => {
                tracing::trace!("CPUID intercept");
                VcpuAction::Continue
            }
            // WHvRunVpExitReasonCanceled = 0x2001 (WHvRunVpCancelReason)
            WHV_RUN_VP_EXIT_REASON(0x2001) => {
                tracing::info!("vCPU run canceled");
                VcpuAction::Stop
            }
            other => {
                tracing::warn!("unhandled WHVP exit reason: {:?}", other);
                VcpuAction::Stop
            }
        }
    }

    impl Drop for WhvpPartition {
        fn drop(&mut self) {
            unsafe {
                if !self.handle.is_invalid() {
                    WHvDeletePartition(self.handle);
                    tracing::debug!("WHVP partition deleted");
                }
            }
        }
    }
}

// =============================================
// 公开 API（平台无关）
// =============================================

/// Windows WHVP 后端内部状态
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
                "WHVP backend requires Windows + 'whvp' feature enabled. \
                 Build with: cargo build --features whvp".into()
            ))
        }
    }
}
// =============================================
// Hypervisor trait 实现
// =============================================

#[async_trait]
impl Hypervisor for WhvpBackend {
    fn probe() -> ProbeResult {
        #[cfg(all(target_os = "windows", feature = "whvp"))]
        {
            use windows::Win32::System::Hypervisor::*;
            use windows::Win32::Foundation::*;
            
            unsafe {
                // 检查 hypervisor 是否存在
                let capability_code = WHV_CAPABILITY_CODE(0); // WHvCapabilityCodeHypervisorPresent
                let mut capability: u32 = 0;
                let mut written_size: u32 = 0;
                
                let hr = WHvGetCapability(
                    capability_code,
                    &mut capability as *mut u32 as *mut _,
                    std::mem::size_of::<u32>() as u32,
                    &mut written_size,
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

            // 创建 partition
            let partition = whvp_impl::WhvpPartition::create(config.cpus as u32)?;
            
            state.partition = Some(partition);
            state.vm_state = VmState::Created;
            
            tracing::info!("WHVP VM created successfully");
            Ok(())
        }

        #[cfg(not(all(target_os = "windows", feature = "whvp")))]
        {
            Err(Error::Vmm("WHVP not available".into()))
        }
    }

    async fn start(&self) -> Result<()> {
        #[cfg(all(target_os = "windows", feature = "whvp"))]
        {
            let mut state = self.state.write();
            
            if state.partition.is_none() {
                return Err(Error::Vmm("VM not created".into()));
            }
            
            state.vm_state = VmState::Running;
            tracing::info!("WHVP VM started");
            Ok(())
        }

        #[cfg(not(all(target_os = "windows", feature = "whvp")))]
        {
            Err(Error::Vmm("WHVP not available".into()))
        }
    }

    async fn stop(&self) -> Result<()> {
        #[cfg(all(target_os = "windows", feature = "whvp"))]
        {
            let mut state = self.state.write();
            state.vm_state = VmState::Stopped;
            tracing::info!("WHVP VM stopped");
            Ok(())
        }

        #[cfg(not(all(target_os = "windows", feature = "whvp")))]
        {
            Err(Error::Vmm("WHVP not available".into()))
        }
    }

    async fn run(&self) -> Result<i32> {
        #[cfg(all(target_os = "windows", feature = "whvp"))]
        {
            use crate::vmm::vcpu::VcpuAction;

            let state_arc = Arc::clone(&self.state);

            // 在专用线程上运行 vCPU（避免阻塞 Tokio executor）
            let result = tokio::task::spawn_blocking(move || -> Result<i32> {
                let state = state_arc.read();

                let partition = state
                    .partition
                    .as_ref()
                    .ok_or_else(|| Error::Vmm("VM not created".into()))?;

                let vcpu_count = partition.vcpu_count();
                tracing::info!("Starting WHVP run loop with {} vCPUs", vcpu_count);

                // 1. 创建所有 vCPU
                for vp_index in 0..vcpu_count {
                    partition.create_vcpu(vp_index)?;
                    // 默认入口地址：0xFFF0（x86 实模式 reset vector）
                    partition.set_initial_registers(vp_index, 0xFFF0)?;
                }

                // 2. vCPU 0 主循环（单核简化实现；多核需多线程）
                let mut exit_code: i32 = 0;
                loop {
                    let exit_ctx = partition.run_vcpu(0)?;
                    match whvp_impl::handle_exit(&exit_ctx) {
                        VcpuAction::Continue => {
                            // 继续运行
                        }
                        VcpuAction::Reschedule => {
                            // 重新调度（短暂让出 CPU）
                            std::thread::yield_now();
                        }
                        VcpuAction::Stop => {
                            tracing::info!("WHVP vCPU 0 exited");
                            break;
                        }
                    }

                    // 检查外部停止信号
                    if state_arc.read().vm_state == crate::vmm::VmState::Stopping {
                        tracing::info!("WHVP VM stop requested");
                        break;
                    }
                }

                // 3. 清理 vCPU
                for vp_index in 0..vcpu_count {
                    partition.delete_vcpu(vp_index);
                }

                Ok(exit_code)
            })
            .await
            .map_err(|e| Error::Vmm(format!("vCPU thread panicked: {e}")))?;

            // 更新状态
            self.state.write().vm_state = VmState::Stopped;
            result
        }

        #[cfg(not(all(target_os = "windows", feature = "whvp")))]
        {
            Err(Error::Vmm("WHVP not available".into()))
        }
    }
}
