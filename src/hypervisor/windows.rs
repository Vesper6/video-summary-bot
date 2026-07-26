//! Windows 后端：WHVP（Windows Hypervisor Platform）
//!
//! windows crate 0.58 API:
//! - `WHvCreatePartition() -> Result<WHV_PARTITION_HANDLE>`
//! - `WHvSetPartitionProperty(handle, code, ptr, size) -> Result<()>`
//! - `WHvMapGpaRange(handle, hva, gpa, size, flags) -> Result<()>`
//! - `WHvRunVirtualProcessor(handle, idx, ctx, size) -> Result<()>`

use async_trait::async_trait;

use crate::error::{Error, Result};
use crate::hypervisor::{Hypervisor, ProbeResult};
use crate::vmm::VmConfig;
#[cfg(all(target_os = "windows", feature = "whvp"))]
use crate::vmm::VmState;
#[cfg(all(target_os = "windows", feature = "whvp"))]
use parking_lot::RwLock;
#[cfg(all(target_os = "windows", feature = "whvp"))]
use std::sync::Arc;

// =============================================
// Windows WHVP 实现（feature-gated）
// =============================================

#[cfg(all(target_os = "windows", feature = "whvp"))]
mod whvp_impl {
    use super::*;
    use windows::Win32::System::Hypervisor::*;

    /// x86 GPR 编号（ModRM reg 字段）→ WHVP 寄存器名。
    pub(super) const GPR_NAMES: [WHV_REGISTER_NAME; 16] = [
        WHvX64RegisterRax,
        WHvX64RegisterRcx,
        WHvX64RegisterRdx,
        WHvX64RegisterRbx,
        WHvX64RegisterRsp,
        WHvX64RegisterRbp,
        WHvX64RegisterRsi,
        WHvX64RegisterRdi,
        WHvX64RegisterR8,
        WHvX64RegisterR9,
        WHvX64RegisterR10,
        WHvX64RegisterR11,
        WHvX64RegisterR12,
        WHvX64RegisterR13,
        WHvX64RegisterR14,
        WHvX64RegisterR15,
    ];

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

                // 启用 xAPIC 模拟：LAPIC 定时器由 WHVP 提供（guest 时钟源），
                // 也是 WHvRequestInterrupt 注入 VirtIO IRQ 的前提。
                let apic_mode: i32 = WHvX64LocalApicEmulationModeXApic.0;
                if let Err(e) = WHvSetPartitionProperty(
                    handle,
                    WHvPartitionPropertyCodeLocalApicEmulationMode,
                    &apic_mode as *const i32 as *const core::ffi::c_void,
                    std::mem::size_of::<i32>() as u32,
                ) {
                    tracing::warn!("LAPIC emulation unavailable ({e}) — virtio IRQ 注入将失效");
                }

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
                WHvMapGpaRange(self.handle, host_ptr, guest_address, size, flags).map_err(|e| {
                    Error::Vmm(format!("WHvMapGpaRange (gpa={:#x}): {e}", guest_address))
                })?;
                Ok(())
            }
        }

        pub fn create_vcpu(&self, vp_index: u32) -> Result<()> {
            unsafe {
                WHvCreateVirtualProcessor(self.handle, vp_index, 0).map_err(|e| {
                    Error::Vmm(format!("WHvCreateVirtualProcessor({}): {e}", vp_index))
                })?;
                Ok(())
            }
        }

        pub fn delete_vcpu(&self, vp_index: u32) {
            unsafe {
                let _ = WHvDeleteVirtualProcessor(self.handle, vp_index);
            }
        }

        /// 设置 vCPU 完整 long-mode 初始状态：
        /// RIP/RSP/RSI/RFLAGS + CR0/CR3/CR4/EFER + CS/DS/ES/SS + GDTR。
        ///
        /// 这是让 Linux bzImage 64-bit 入口能执行的关键。
        #[allow(clippy::too_many_arguments)]
        pub fn set_long_mode_entry(
            &self,
            vp_index: u32,
            rip: u64,
            rsp: u64,
            rsi: u64,
            cr0: u64,
            cr3: u64,
            cr4: u64,
            efer: u64,
            gdt_base: u64,
            gdt_limit: u16,
        ) -> Result<()> {
            use crate::vmm::loader::{SEL_CODE64, SEL_DATA64};
            unsafe {
                // 64-bit code segment：base=0 limit=0xFFFFF, L=1
                // Attributes 布局（WHV_X64_SEGMENT_REGISTER Anonymous.Attributes）:
                //   bit0-3 Type, bit4 S(non-system), bit5-6 DPL, bit7 P,
                //   bit8-11 Limit[19:16], bit12 Avl, bit13 L(long), bit14 D/B, bit15 G
                // code64: Type=0xB(exec/read/accessed) S=1 P=1 L=1 G=1 → 0xA09B
                let code_attr: u16 = 0xA09B;
                // data:   Type=0x3(read/write/accessed) S=1 P=1 D/B=1 G=1 → 0xC093
                let data_attr: u16 = 0xC093;

                let make_seg = |sel: u16, attr: u16| -> WHV_X64_SEGMENT_REGISTER {
                    let mut seg: WHV_X64_SEGMENT_REGISTER = std::mem::zeroed();
                    seg.Base = 0;
                    seg.Limit = 0xF_FFFF;
                    seg.Selector = sel;
                    seg.Anonymous.Attributes = attr;
                    seg
                };

                let cs = make_seg(SEL_CODE64, code_attr);
                let ds = make_seg(SEL_DATA64, data_attr);

                let mut gdtr: WHV_X64_TABLE_REGISTER = std::mem::zeroed();
                gdtr.Base = gdt_base;
                gdtr.Limit = gdt_limit;

                let reg_names: [WHV_REGISTER_NAME; 13] = [
                    WHvX64RegisterRip,
                    WHvX64RegisterRsp,
                    WHvX64RegisterRsi,
                    WHvX64RegisterRflags,
                    WHvX64RegisterCr0,
                    WHvX64RegisterCr3,
                    WHvX64RegisterCr4,
                    WHvX64RegisterEfer,
                    WHvX64RegisterCs,
                    WHvX64RegisterDs,
                    WHvX64RegisterEs,
                    WHvX64RegisterSs,
                    WHvX64RegisterGdtr,
                ];
                let reg_values = [
                    WHV_REGISTER_VALUE { Reg64: rip },
                    WHV_REGISTER_VALUE { Reg64: rsp },
                    WHV_REGISTER_VALUE { Reg64: rsi },
                    WHV_REGISTER_VALUE { Reg64: 0x0000_0002 },
                    WHV_REGISTER_VALUE { Reg64: cr0 },
                    WHV_REGISTER_VALUE { Reg64: cr3 },
                    WHV_REGISTER_VALUE { Reg64: cr4 },
                    WHV_REGISTER_VALUE { Reg64: efer },
                    WHV_REGISTER_VALUE { Segment: cs },
                    WHV_REGISTER_VALUE { Segment: ds },
                    WHV_REGISTER_VALUE { Segment: ds },
                    WHV_REGISTER_VALUE { Segment: ds },
                    WHV_REGISTER_VALUE { Table: gdtr },
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

        /// 读取单个 64-bit 寄存器。
        pub fn get_reg(&self, vp_index: u32, name: WHV_REGISTER_NAME) -> Result<u64> {
            unsafe {
                let names = [name];
                let mut values = [WHV_REGISTER_VALUE::default()];
                WHvGetVirtualProcessorRegisters(
                    self.handle,
                    vp_index,
                    names.as_ptr(),
                    1,
                    values.as_mut_ptr(),
                )
                .map_err(|e| Error::Vmm(format!("get_reg: {e}")))?;
                Ok(values[0].Reg64)
            }
        }

        /// 向 vCPU 注入固定向量中断（边沿触发，物理目标 APIC 0）。
        /// 需要分区已启用 LAPIC 模拟。
        pub fn request_interrupt(&self, vector: u32) -> Result<()> {
            unsafe {
                let ctrl = WHV_INTERRUPT_CONTROL {
                    // bitfield：Type(8)=Fixed(0) | DestinationMode(4)=Physical(0)
                    //           | TriggerMode(4)=Edge(0)
                    _bitfield: WHvX64InterruptTypeFixed.0 as u64,
                    Destination: 0,
                    Vector: vector,
                };
                WHvRequestInterrupt(
                    self.handle,
                    &ctrl,
                    std::mem::size_of::<WHV_INTERRUPT_CONTROL>() as u32,
                )
                .map_err(|e| Error::Vmm(format!("WHvRequestInterrupt(v={vector:#x}): {e}")))
            }
        }

        /// 设置单个 64-bit 寄存器。
        pub fn set_reg(&self, vp_index: u32, name: WHV_REGISTER_NAME, value: u64) -> Result<()> {
            unsafe {
                let names = [name];
                let values = [WHV_REGISTER_VALUE { Reg64: value }];
                WHvSetVirtualProcessorRegisters(
                    self.handle,
                    vp_index,
                    names.as_ptr(),
                    1,
                    values.as_ptr(),
                )
                .map_err(|e| Error::Vmm(format!("set_reg: {e}")))?;
                Ok(())
            }
        }

        /// 把 guest 虚拟地址翻译成 guest 物理地址（走 guest 页表）。
        pub fn translate_gva(&self, vp_index: u32, gva: u64) -> Option<u64> {
            unsafe {
                let mut result: WHV_TRANSLATE_GVA_RESULT = std::mem::zeroed();
                let mut gpa: u64 = 0;
                let hr = WHvTranslateGva(
                    self.handle,
                    vp_index,
                    gva,
                    WHvTranslateGvaFlagValidateRead,
                    &mut result,
                    &mut gpa,
                );
                // ResultCode 0 = Success
                if hr.is_ok() && result.ResultCode.0 == 0 {
                    // gpa 是页对齐的，加上页内偏移
                    Some((gpa & !0xFFF) | (gva & 0xFFF))
                } else {
                    None
                }
            }
        }

        /// 完成 I/O 指令：推进 RIP 越过指令，可选写回 RAX。
        pub fn complete_io(&self, vp_index: u32, next_rip: u64, rax: Option<u64>) -> Result<()> {
            unsafe {
                if let Some(rax_val) = rax {
                    let names = [WHvX64RegisterRip, WHvX64RegisterRax];
                    let values = [
                        WHV_REGISTER_VALUE { Reg64: next_rip },
                        WHV_REGISTER_VALUE { Reg64: rax_val },
                    ];
                    WHvSetVirtualProcessorRegisters(
                        self.handle,
                        vp_index,
                        names.as_ptr(),
                        2,
                        values.as_ptr(),
                    )
                    .map_err(|e| Error::Vmm(format!("complete_io: {e}")))?;
                } else {
                    let names = [WHvX64RegisterRip];
                    let values = [WHV_REGISTER_VALUE { Reg64: next_rip }];
                    WHvSetVirtualProcessorRegisters(
                        self.handle,
                        vp_index,
                        names.as_ptr(),
                        1,
                        values.as_ptr(),
                    )
                    .map_err(|e| Error::Vmm(format!("complete_io: {e}")))?;
                }
                Ok(())
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

    /// 模拟一次落在 VirtIO MMIO 区域的内存访问：
    /// 解码指令 → 读/写设备 → 写回寄存器 → 推进 RIP。
    /// 返回 `true` 表示已处理（RIP 已推进），`false` 表示无法解码。
    pub(super) fn handle_mmio_access(
        partition: &WhvpPartition,
        handler: &Arc<dyn crate::hypervisor::VmIoHandler>,
        gpa: u64,
        rip: u64,
        insn_bytes: &[u8],
    ) -> Result<bool> {
        use crate::vmm::mmio_insn::{decode_mmio_insn, MmioOp};

        let Some(insn) = decode_mmio_insn(insn_bytes) else {
            tracing::warn!(
                "MMIO 指令无法解码 @rip={rip:#x} gpa={gpa:#x} bytes={:02x?}",
                &insn_bytes[..insn_bytes.len().min(8)]
            );
            return Ok(false);
        };
        let next_rip = rip + insn.len as u64;
        let size = insn.access_size;

        match insn.op {
            MmioOp::Load { reg, zero_extend } => {
                let val = handler.mmio_read(gpa, size).unwrap_or(0);
                let reg_name = GPR_NAMES[reg as usize];
                let new = if zero_extend {
                    val
                } else {
                    // 8/16 位普通 mov：仅覆盖低位，保留高位
                    let old = partition.get_reg(0, reg_name)?;
                    let mask = if size == 1 { 0xFF } else { 0xFFFF };
                    (old & !mask) | (val & mask)
                };
                partition.set_reg(0, reg_name, new)?;
                partition.set_reg(0, WHvX64RegisterRip, next_rip)?;
            }
            MmioOp::Store { reg } => {
                let val = partition.get_reg(0, GPR_NAMES[reg as usize])?;
                handler.mmio_write(gpa, val, size);
                partition.set_reg(0, WHvX64RegisterRip, next_rip)?;
            }
            MmioOp::StoreImm { imm } => {
                handler.mmio_write(gpa, imm, size);
                partition.set_reg(0, WHvX64RegisterRip, next_rip)?;
            }
        }
        Ok(true)
    }
}

// =============================================
// 公开 API
// =============================================

#[cfg(all(target_os = "windows", feature = "whvp"))]
/// vCPU 0 完整 long-mode 入口状态。
#[derive(Clone, Copy)]
struct VcpuEntry {
    rip: u64,
    rsp: u64,
    rsi: u64,
    cr0: u64,
    cr3: u64,
    cr4: u64,
    efer: u64,
    gdt_base: u64,
    gdt_limit: u16,
}

#[cfg(all(target_os = "windows", feature = "whvp"))]
struct WhvpState {
    partition: Option<whvp_impl::WhvpPartition>,
    vm_state: VmState,
    /// vCPU 0 入口状态；None 表示尚未设置
    vcpu_entry: Option<VcpuEntry>,
    /// Guest RAM 宿主指针（用于读取指令做长度解码）
    ram_ptr: usize,
    ram_size: u64,
    /// 停止标志：run 循环每轮检查（GUI/API 停止时置位）
    stop_flag: Arc<std::sync::atomic::AtomicBool>,
    /// 设备 I/O 分发钩子（PIT / VirtIO MMIO / IRQ）
    io_handler: Option<Arc<dyn crate::hypervisor::VmIoHandler>>,
}

/// Windows WHVP（Windows Hypervisor Platform）后端。
pub struct WhvpBackend {
    #[cfg(all(target_os = "windows", feature = "whvp"))]
    state: Arc<RwLock<WhvpState>>,
}

impl WhvpBackend {
    /// 创建 WHVP 后端（需要 Windows + `whvp` feature）。
    pub fn new() -> Result<Self> {
        #[cfg(all(target_os = "windows", feature = "whvp"))]
        {
            Ok(Self {
                state: Arc::new(RwLock::new(WhvpState {
                    partition: None,
                    vm_state: VmState::Created,
                    vcpu_entry: None,
                    ram_ptr: 0,
                    ram_size: 0,
                    stop_flag: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                    io_handler: None,
                })),
            })
        }
        #[cfg(not(all(target_os = "windows", feature = "whvp")))]
        {
            Err(Error::Vmm(
                "WHVP backend requires Windows + 'whvp' feature. \
                 Build with: cargo build --features whvp"
                    .into(),
            ))
        }
    }

    /// 把宿主 RAM 映射到客户机物理地址空间。
    ///
    /// `hva` 必须是 4KB 对齐的宿主虚拟地址；
    /// `gpa` + `size` 必须在 partition 范围内。
    pub fn map_ram(&self, hva: *mut u8, gpa: u64, size: u64) -> Result<()> {
        #[cfg(all(target_os = "windows", feature = "whvp"))]
        {
            let mut state = self.state.write();
            let partition = state
                .partition
                .as_ref()
                .ok_or_else(|| Error::Vmm("VM not created".into()))?;
            partition.map_gpa_range(
                hva as *mut core::ffi::c_void,
                gpa,
                size,
                windows::Win32::System::Hypervisor::WHV_MAP_GPA_RANGE_FLAGS(0x0000_0007), // Read|Write|Execute
            )?;
            // 记录 RAM 指针（gpa=0 的主映射），用于指令长度解码
            if gpa == 0 {
                state.ram_ptr = hva as usize;
                state.ram_size = size;
            }
            Ok(())
        }
        #[cfg(not(all(target_os = "windows", feature = "whvp")))]
        {
            let _ = (hva, gpa, size);
            Err(Error::Vmm("WHVP not available".into()))
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
                ProbeResult::err(
                    "WHVP",
                    "feature 'whvp' not enabled - build with --features whvp",
                )
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

    fn request_stop(&self) {
        #[cfg(all(target_os = "windows", feature = "whvp"))]
        {
            self.state
                .read()
                .stop_flag
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    fn set_io_handler(&self, _handler: std::sync::Arc<dyn crate::hypervisor::VmIoHandler>) {
        #[cfg(all(target_os = "windows", feature = "whvp"))]
        {
            self.state.write().io_handler = Some(_handler);
        }
    }

    fn map_ram(&self, hva: *mut u8, gpa: u64, size: u64) -> Result<()> {
        #[cfg(all(target_os = "windows", feature = "whvp"))]
        {
            self.map_ram(hva, gpa, size)
        }
        #[cfg(not(all(target_os = "windows", feature = "whvp")))]
        {
            let _ = (hva, gpa, size);
            Err(Error::Vmm("WHVP not available".into()))
        }
    }

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
    ) -> Result<()> {
        #[cfg(all(target_os = "windows", feature = "whvp"))]
        {
            let mut state = self.state.write();
            state.vcpu_entry = Some(VcpuEntry {
                rip,
                rsp,
                rsi,
                cr0,
                cr3,
                cr4,
                efer,
                gdt_base,
                gdt_limit,
            });
            Ok(())
        }
        #[cfg(not(all(target_os = "windows", feature = "whvp")))]
        {
            let _ = (rip, rsp, rsi, cr0, cr3, cr4, efer, gdt_base, gdt_limit);
            Err(Error::Vmm("WHVP not available".into()))
        }
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
                config.name,
                config.cpus,
                config.memory_mb
            );
            let partition = whvp_impl::WhvpPartition::create(config.cpus as u32)?;
            state.partition = Some(partition);
            state.vm_state = VmState::Created;
            Ok(())
        }

        #[cfg(not(all(target_os = "windows", feature = "whvp")))]
        {
            let _ = config;
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
                let partition = state
                    .partition
                    .as_ref()
                    .ok_or_else(|| Error::Vmm("VM not created".into()))?;
                let vcpu_count = partition.vcpu_count();
                tracing::info!("WHVP run loop starting, {} vCPUs", vcpu_count);

                let ram_ptr = state.ram_ptr;
                let ram_size = state.ram_size;
                let stop_flag = Arc::clone(&state.stop_flag);
                let io_handler = state.io_handler.clone();
                stop_flag.store(false, std::sync::atomic::Ordering::SeqCst);

                let entry = state.vcpu_entry.ok_or_else(|| {
                    Error::Vmm("vcpu_entry not set - call set_vcpu_entry() before run()".into())
                })?;

                // 从 guest RIP 读取指令字节，解码 I/O 指令长度。
                // RIP 可能是虚拟地址（内核跳到高半区后），需先翻译成物理地址。
                let decode_io_len = |part: &whvp_impl::WhvpPartition, rip: u64| -> u64 {
                    // 翻译 GVA → GPA（低地址 identity-map 时翻译结果==rip）
                    let phys = part.translate_gva(0, rip).unwrap_or(rip);
                    if ram_ptr == 0 || phys >= ram_size {
                        return 1;
                    }
                    let read = |off: u64| -> u8 {
                        if off >= ram_size {
                            return 0;
                        }
                        unsafe { *((ram_ptr + off as usize) as *const u8) }
                    };
                    let mut i = 0u64;
                    let mut len = 0u64;
                    loop {
                        let b = read(phys + i);
                        if matches!(b, 0x66 | 0x67 | 0xF0 | 0xF2 | 0xF3 | 0x40..=0x4F) {
                            // 前缀（含 REX 0x40-0x4F）
                            i += 1;
                            len += 1;
                            if i > 4 {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    let op = read(phys + i);
                    len += match op {
                        0xE4..=0xE7 => 2, // in/out imm8
                        _ => 1,           // in/out dx (0xEC-0xEF) 及兜底
                    };
                    len
                };

                // 只启动 vCPU 0（BSP）；其余 AP 由内核通过 INIT-SIPI 唤醒
                partition.create_vcpu(0)?;
                partition.set_long_mode_entry(
                    0,
                    entry.rip,
                    entry.rsp,
                    entry.rsi,
                    entry.cr0,
                    entry.cr3,
                    entry.cr4,
                    entry.efer,
                    entry.gdt_base,
                    entry.gdt_limit,
                )?;
                tracing::info!(
                    "vCPU 0 long-mode entry: rip={:#x} rsp={:#x} rsi={:#x} cr3={:#x}",
                    entry.rip,
                    entry.rsp,
                    entry.rsi,
                    entry.cr3
                );

                let mut exit_code: i32 = 0;
                let mut step_count = 0u64;
                // 16550 UART（COM1 @ 0x3F8）有状态寄存器
                struct Uart {
                    ier: u8,
                    fcr: u8,
                    lcr: u8,
                    mcr: u8,
                    scr: u8,
                    dll: u8,
                    dlm: u8,
                }
                let mut uart = Uart {
                    ier: 0,
                    fcr: 0,
                    lcr: 0,
                    mcr: 0,
                    scr: 0,
                    dll: 0,
                    dlm: 0,
                };
                let mut uart_line: Vec<u8> = Vec::with_capacity(256);
                // 退出原因直方图（调试用）
                let mut reason_hist: std::collections::HashMap<i32, u64> =
                    std::collections::HashMap::new();
                let mut io_ports_seen: std::collections::HashMap<u16, u64> =
                    std::collections::HashMap::new();
                // VirtIO IRQ → 中断向量映射：IRQ n → vector 0x30+n（避开 CPU 异常 0-31）
                const IRQ_VECTOR_BASE: u32 = 0x30;
                loop {
                    // GUI/API 请求停止：优雅退出
                    if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
                        tracing::info!("vCPU 0 收到停止请求，退出 run 循环");
                        break;
                    }
                    // 设备有待注入的中断：通过 LAPIC 注入固定向量
                    if let Some(h) = &io_handler {
                        if let Some(irq) = h.take_pending_irq() {
                            if let Err(e) =
                                partition.request_interrupt(IRQ_VECTOR_BASE + irq as u32)
                            {
                                tracing::trace!("irq inject failed: {e}");
                            }
                        }
                    }
                    let exit_ctx = partition.run_vcpu(0)?;
                    step_count += 1;
                    *reason_hist.entry(exit_ctx.ExitReason.0).or_insert(0) += 1;
                    if exit_ctx.ExitReason.0 == 2 {
                        let p = unsafe { exit_ctx.Anonymous.IoPortAccess.PortNumber };
                        *io_ports_seen.entry(p).or_insert(0) += 1;
                    }
                    let rip = exit_ctx.VpContext.Rip;
                    // WHV_RUN_VP_EXIT_REASON 常量：
                    //  1=MemoryAccess 2=IoPort 4=UnrecoverableException
                    //  8=Halt 6=UnsupportedFeature 4097=Cpuid 4096=MsrAccess
                    match exit_ctx.ExitReason.0 {
                        // X64Halt
                        8 => {
                            tracing::info!("vCPU 0 halted @ rip={:#x}", rip);
                            break;
                        }
                        // MemoryAccess (MMIO 或缺页)
                        1 => {
                            let mem = unsafe { &exit_ctx.Anonymous.MemoryAccess };
                            let gpa = mem.Gpa;
                            let mut handled = false;

                            // 落在 VirtIO MMIO 区域 → 解码指令并模拟设备访问
                            if let Some(h) = &io_handler {
                                if gpa >= crate::devices::VirtioMmioBus::BASE_ADDR {
                                    // WHVP 已附带指令字节；不足时从 guest RAM 补读
                                    let mut ibytes = mem.InstructionBytes.to_vec();
                                    if mem.InstructionByteCount == 0 {
                                        if let Some(phys) = partition.translate_gva(0, rip) {
                                            if phys < ram_size && ram_ptr != 0 {
                                                ibytes = (0..16)
                                                    .map(|k| {
                                                        let off = phys + k;
                                                        if off < ram_size {
                                                            unsafe {
                                                                *((ram_ptr + off as usize)
                                                                    as *const u8)
                                                            }
                                                        } else {
                                                            0
                                                        }
                                                    })
                                                    .collect();
                                            }
                                        }
                                    }
                                    handled = whvp_impl::handle_mmio_access(
                                        partition, h, gpa, rip, &ibytes,
                                    )?;
                                }
                            }

                            if !handled && (step_count < 20 || step_count % 20000 == 0) {
                                tracing::info!(
                                    "vCPU MemoryAccess (unhandled): rip={:#x} gpa={:#x} ({} steps)",
                                    rip,
                                    gpa,
                                    step_count
                                );
                            }
                        }
                        // IoPortAccess — 模拟 COM1 (0x3F8) UART
                        2 => {
                            let io = unsafe { &exit_ctx.Anonymous.IoPortAccess };
                            let port = io.PortNumber;
                            // AccessInfo bitfield: bit0 = IsWrite, bit1-2 = AccessSize
                            let access_bits = unsafe { io.AccessInfo.Anonymous._bitfield };
                            let is_write = (access_bits & 1) != 0;
                            let mut insn_len = io.InstructionByteCount as u64;
                            if insn_len == 0 {
                                // WHVP 未解码：自己从 guest RAM 读指令算长度
                                insn_len = decode_io_len(partition, rip);
                            }
                            let next_rip = rip + insn_len;

                            // 16550 UART（COM1 base=0x3F8）有状态模拟
                            if (0x3F8..=0x3FF).contains(&port) {
                                let reg = port - 0x3F8; // 0..7
                                let dlab = (uart.lcr & 0x80) != 0;
                                if is_write {
                                    let byte = (io.Rax & 0xFF) as u8;
                                    match reg {
                                        0 if !dlab => {
                                            // THR：输出字符
                                            if byte == b'\n' {
                                                let line = String::from_utf8_lossy(&uart_line);
                                                crate::guest_log::emit(&line);
                                                tracing::info!(target: "guest", "{}", line);
                                                uart_line.clear();
                                            } else if byte != b'\r' {
                                                uart_line.push(byte);
                                            }
                                        }
                                        0 => uart.dll = byte, // DLL (DLAB=1)
                                        1 if dlab => uart.dlm = byte, // DLM (DLAB=1)
                                        1 => uart.ier = byte, // IER
                                        2 => uart.fcr = byte, // FCR
                                        3 => uart.lcr = byte, // LCR
                                        4 => uart.mcr = byte, // MCR
                                        7 => uart.scr = byte, // SCR
                                        _ => {}
                                    }
                                    partition.complete_io(0, next_rip, None)?;
                                } else {
                                    let val: u8 = match reg {
                                        0 if !dlab => 0,       // RBR：无输入
                                        0 => uart.dll,         // DLL
                                        1 if dlab => uart.dlm, // DLM
                                        1 => uart.ier,         // IER
                                        2 => 0x01,             // IIR：无中断挂起
                                        3 => uart.lcr,         // LCR（关键：回读）
                                        4 => uart.mcr,         // MCR
                                        5 => 0x60,             // LSR：THR+TX empty
                                        6 => 0xB0,             // MSR：DCD+DSR+CTS
                                        7 => uart.scr,         // SCR
                                        _ => 0,
                                    };
                                    let rax = (io.Rax & !0xFF) | val as u64;
                                    partition.complete_io(0, next_rip, Some(rax))?;
                                }
                            } else if io_handler
                                .as_ref()
                                .map(|h| {
                                    // PIT 等平台设备端口
                                    let sz = 1u8;
                                    if is_write {
                                        h.pio_write(port, (io.Rax & 0xFF) as u32, sz)
                                    } else {
                                        false // 读在下面单独处理以拿回值
                                    }
                                })
                                .unwrap_or(false)
                            {
                                partition.complete_io(0, next_rip, None)?;
                            } else if !is_write
                                && io_handler.is_some()
                                && crate::devices::pit::Pit::handles_port(port)
                            {
                                let val = io_handler
                                    .as_ref()
                                    .and_then(|h| h.pio_read(port, 1))
                                    .unwrap_or(0xFF)
                                    & 0xFF;
                                let rax = (io.Rax & !0xFF) | val as u64;
                                partition.complete_io(0, next_rip, Some(rax))?;
                            } else {
                                // 其他端口：读返回 0xFF，写忽略
                                if is_write {
                                    partition.complete_io(0, next_rip, None)?;
                                } else {
                                    let rax = io.Rax | 0xFF;
                                    partition.complete_io(0, next_rip, Some(rax))?;
                                }
                            }
                        }
                        // UnrecoverableException（三重故障）
                        4 => {
                            tracing::error!(
                                "vCPU 0 UNRECOVERABLE EXCEPTION @ rip={:#x} (step {}) - 三重故障",
                                rip,
                                step_count
                            );
                            exit_code = -4;
                            break;
                        }
                        // UnsupportedFeature
                        6 => {
                            tracing::error!("vCPU 0 unsupported feature @ rip={:#x}", rip);
                            exit_code = -6;
                            break;
                        }
                        // Cpuid
                        4097 => {
                            if step_count < 5 {
                                tracing::info!("vCPU CPUID @ rip={:#x}", rip);
                            }
                        }
                        // MsrAccess
                        4096 => {
                            if step_count < 10 {
                                tracing::info!("vCPU MSR access @ rip={:#x}", rip);
                            }
                        }
                        other => {
                            tracing::warn!(
                                "vCPU 0 exit reason {} @ rip={:#x} (step {})",
                                other,
                                rip,
                                step_count
                            );
                            if step_count > 50 {
                                exit_code = other;
                                break;
                            }
                        }
                    }
                    // 硬上限仅作最后防线（真正的时间边界由 boot.rs 的 timeout +
                    // stop_flag 控制）。设得足够高，让内核有机会跑到 userspace。
                    if step_count > 500_000_000 {
                        tracing::warn!("vCPU 0 步数达到硬上限（5 亿），停止");
                        break;
                    }
                }

                // 打印退出原因直方图
                tracing::info!("=== exit reason histogram (total {} steps) ===", step_count);
                let mut reasons: Vec<_> = reason_hist.iter().collect();
                reasons.sort_by(|a, b| b.1.cmp(a.1));
                for (reason, count) in reasons {
                    tracing::info!("  reason {} → {} 次", reason, count);
                }
                if !io_ports_seen.is_empty() {
                    tracing::info!("=== I/O ports seen ===");
                    let mut ports: Vec<_> = io_ports_seen.iter().collect();
                    ports.sort_by(|a, b| b.1.cmp(a.1));
                    for (port, count) in ports.iter().take(15) {
                        tracing::info!("  port {:#x} → {} 次", port, count);
                    }
                }

                partition.delete_vcpu(0);

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
