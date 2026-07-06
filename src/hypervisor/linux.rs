//! Linux 后端：KVM（Kernel-based Virtual Machine）
//!
//! KVM 通过 `/dev/kvm` 设备暴露，`kvm-ioctls` crate 提供安全封装：
//! - `Kvm::new()` — 打开 /dev/kvm，获取 Kvm fd
//! - `Kvm::create_vm()` — KVM_CREATE_VM，返回 VmFd
//! - `VmFd::set_user_memory_region()` — KVM_SET_USER_MEMORY_REGION
//! - `VmFd::create_vcpu()` — KVM_CREATE_VCPU，返回 VcpuFd
//! - `VcpuFd::run()` — KVM_RUN，返回 VcpuExit

use async_trait::async_trait;
#[cfg(all(target_os = "linux", feature = "kvm"))]
use std::sync::Arc;
#[cfg(all(target_os = "linux", feature = "kvm"))]
use parking_lot::RwLock;

use crate::error::{Error, Result};
use crate::hypervisor::{Hypervisor, ProbeResult};
use crate::vmm::VmConfig;
#[cfg(all(target_os = "linux", feature = "kvm"))]
use crate::vmm::VmState;

// =============================================
// KVM 实现（feature-gated：仅 Linux + kvm feature）
// =============================================

#[cfg(all(target_os = "linux", feature = "kvm"))]
mod kvm_impl {
    use super::*;
    use kvm_ioctls::{Kvm, VmFd, VcpuFd, VcpuExit};
    use kvm_bindings::{
        kvm_userspace_memory_region,
        kvm_regs,
        kvm_sregs,
        KVM_MEM_LOG_DIRTY_PAGES,
    };

    /// 页对齐的宿主内存块（用于客户机 RAM）。
    pub(super) struct MemorySlot {
        /// 宿主虚拟地址（页对齐）
        pub hva: *mut u8,
        /// 客户机物理地址起始
        pub gpa: u64,
        /// 大小（字节，必须 4KB 对齐）
        pub size: usize,
        /// slot 编号
        pub slot: u32,
    }

    impl MemorySlot {
        pub fn allocate(gpa: u64, size: usize, slot: u32) -> Result<Self> {
            if size == 0 || size % 4096 != 0 {
                return Err(Error::Vmm("memory size must be 4KB aligned".into()));
            }
            // mmap 匿名页（保证页对齐）
            let ptr = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    size,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED | libc::MAP_ANONYMOUS,
                    -1,
                    0,
                )
            };
            if ptr == libc::MAP_FAILED {
                return Err(Error::Vmm(format!(
                    "mmap failed for guest RAM at gpa={:#x}, size={:#x}: {}",
                    gpa, size, std::io::Error::last_os_error()
                )));
            }
            Ok(Self {
                hva: ptr as *mut u8,
                gpa,
                size,
                slot,
            })
        }
    }

    impl Drop for MemorySlot {
        fn drop(&mut self) {
            unsafe {
                if !self.hva.is_null() {
                    libc::munmap(self.hva as *mut libc::c_void, self.size);
                }
            }
        }
    }

    // SAFETY: MemorySlot 通过 RwLock 串行访问
    unsafe impl Send for MemorySlot {}
    unsafe impl Sync for MemorySlot {}

    /// KVM VM 包装（RAII 管理 VmFd + vCPU fds + 内存）
    pub(super) struct KvmVm {
        pub vm_fd: VmFd,
        pub vcpus: Vec<VcpuFd>,
        pub memory_slots: Vec<MemorySlot>,
        pub vcpu_count: u32,
    }

    impl KvmVm {
        /// 创建 KVM VM 并分配客户机内存。
        pub fn create(kvm: &Kvm, config: &VmConfig) -> Result<Self> {
            // 创建 VM fd
            let vm_fd = kvm
                .create_vm()
                .map_err(|e| Error::Vmm(format!("KVM_CREATE_VM failed: {e}")))?;

            // 分配客户机 RAM（从 GPA 0 开始）
            let ram_size = config.memory_mb as usize * 1024 * 1024;
            let mut memory_slots = Vec::new();
            let slot = MemorySlot::allocate(0, ram_size, 0)?;

            // 注册内存区域到 KVM
            let mem_region = kvm_userspace_memory_region {
                slot: 0,
                guest_phys_addr: slot.gpa,
                memory_size: slot.size as u64,
                userspace_addr: slot.hva as u64,
                flags: 0,
            };
            unsafe {
                vm_fd
                    .set_user_memory_region(mem_region)
                    .map_err(|e| Error::Vmm(format!("KVM_SET_USER_MEMORY_REGION failed: {e}")))?;
            }
            tracing::debug!(
                "KVM memory slot 0: GPA {:#x}..{:#x} ({}MB)",
                0,
                ram_size,
                config.memory_mb
            );
            memory_slots.push(slot);

            tracing::info!(
                "KVM VM created: {} (cpus={}, memory={}MB)",
                config.name, config.cpus, config.memory_mb
            );
            Ok(Self {
                vm_fd,
                vcpus: Vec::new(),
                memory_slots,
                vcpu_count: config.cpus as u32,
            })
        }

        /// 创建所有 vCPU，并设置初始寄存器。
        pub fn create_vcpus(&mut self, entry_rip: u64) -> Result<()> {
            for id in 0..self.vcpu_count {
                let vcpu = self
                    .vm_fd
                    .create_vcpu(id as u64)
                    .map_err(|e| Error::Vmm(format!("KVM_CREATE_VCPU({id}) failed: {e}")))?;

                setup_long_mode_regs(&vcpu, entry_rip)?;
                self.vcpus.push(vcpu);
                tracing::debug!("vCPU {} created (RIP={:#x})", id, entry_rip);
            }
            Ok(())
        }

        /// vCPU 0 运行循环（同步，在 spawn_blocking 线程中调用）。
        pub fn run_vcpu0(&self) -> Result<i32> {
            let vcpu = self.vcpus.get(0).ok_or_else(|| Error::Vmm("no vCPUs".into()))?;
            loop {
                match vcpu.run() {
                    Ok(exit) => match handle_kvm_exit(&exit) {
                        KvmAction::Continue => {}
                        KvmAction::Halt => {
                            tracing::info!("vCPU 0 halted");
                            return Ok(0);
                        }
                        KvmAction::Shutdown => {
                            tracing::info!("guest shutdown");
                            return Ok(0);
                        }
                        KvmAction::Error(code) => {
                            tracing::error!("vCPU 0 fatal exit, code={code}");
                            return Ok(code);
                        }
                    },
                    Err(e) => {
                        return Err(Error::Vmm(format!("KVM_RUN failed: {e}")));
                    }
                }
            }
        }
    }

    /// vCPU 动作。
    enum KvmAction {
        Continue,
        Halt,
        Shutdown,
        Error(i32),
    }

    /// 处理 KVM exit reason。
    fn handle_kvm_exit(exit: &VcpuExit) -> KvmAction {
        match exit {
            VcpuExit::IoIn(port, data) => {
                tracing::trace!("I/O IN port={:#x} size={}", port, data.len());
                // 默认返回 0xFF（未知设备）
                for b in data.iter_mut() {
                    *b = 0xFF;
                }
                KvmAction::Continue
            }
            VcpuExit::IoOut(port, data) => {
                tracing::trace!(
                    "I/O OUT port={:#x} data={:?}",
                    port,
                    &data[..data.len().min(4)]
                );
                // 串口输出（0x3F8 = COM1）
                if *port == 0x3F8 {
                    print!("{}", char::from(data[0]));
                }
                KvmAction::Continue
            }
            VcpuExit::MmioRead(addr, data) => {
                tracing::trace!("MMIO read addr={:#x} size={}", addr, data.len());
                KvmAction::Continue
            }
            VcpuExit::MmioWrite(addr, data) => {
                tracing::trace!(
                    "MMIO write addr={:#x} data={:?}",
                    addr,
                    &data[..data.len().min(4)]
                );
                KvmAction::Continue
            }
            VcpuExit::Hlt => KvmAction::Halt,
            VcpuExit::Shutdown => KvmAction::Shutdown,
            VcpuExit::SystemEvent(event_type, _flags) => {
                tracing::info!("system event: {}", event_type);
                KvmAction::Shutdown
            }
            VcpuExit::FailEntry(reason, _vcpu_id) => {
                tracing::error!("KVM fail entry: reason={:#x}", reason);
                KvmAction::Error(1)
            }
            VcpuExit::InternalError => {
                tracing::error!("KVM internal error");
                KvmAction::Error(1)
            }
            other => {
                tracing::warn!("unhandled KVM exit: {:?}", other);
                KvmAction::Continue
            }
        }
    }

    /// 配置 vCPU 为 64-bit long mode（最简配置，不带分页）。
    fn setup_long_mode_regs(vcpu: &VcpuFd, entry_rip: u64) -> Result<()> {
        // 通用寄存器
        let mut regs = vcpu
            .get_regs()
            .map_err(|e| Error::Vmm(format!("KVM_GET_REGS failed: {e}")))?;
        regs.rip = entry_rip;
        regs.rflags = 0x0000_0002; // 保留位
        regs.rsp = 0x0000_8000;   // 初始栈指针
        vcpu.set_regs(&regs)
            .map_err(|e| Error::Vmm(format!("KVM_SET_REGS failed: {e}")))?;

        // 系统寄存器（进入实模式或保护模式）
        let mut sregs = vcpu
            .get_sregs()
            .map_err(|e| Error::Vmm(format!("KVM_GET_SREGS failed: {e}")))?;
        // CS: 代码段 base=0, limit=0xFFFF
        sregs.cs.base = 0;
        sregs.cs.limit = 0xFFFF;
        sregs.cs.selector = 0;
        // CR0: PE=1 (保护模式), PG=0 (无分页)
        sregs.cr0 = 0x0000_0001;
        vcpu.set_sregs(&sregs)
            .map_err(|e| Error::Vmm(format!("KVM_SET_SREGS failed: {e}")))?;

        tracing::debug!("vCPU registers set: RIP={:#x} RSP={:#x}", entry_rip, 0x8000_u64);
        Ok(())
    }
}

// =============================================
// 公开 API（平台无关）
// =============================================

#[cfg(all(target_os = "linux", feature = "kvm"))]
struct KvmState {
    vm: Option<kvm_impl::KvmVm>,
    vm_state: VmState,
}

pub struct KvmBackend {
    #[cfg(all(target_os = "linux", feature = "kvm"))]
    state: Arc<RwLock<KvmState>>,
    #[cfg(all(target_os = "linux", feature = "kvm"))]
    kvm: Arc<kvm_ioctls::Kvm>,
}

impl KvmBackend {
    pub fn new() -> Result<Self> {
        #[cfg(all(target_os = "linux", feature = "kvm"))]
        {
            let kvm = kvm_ioctls::Kvm::new()
                .map_err(|e| Error::Vmm(format!("failed to open /dev/kvm: {e}")))?;

            // 验证 API 版本
            let version = kvm.get_api_version();
            if version != 12 {
                return Err(Error::Vmm(format!(
                    "unexpected KVM API version: {version} (expected 12)"
                )));
            }
            tracing::info!("KVM API version: {}", version);

            Ok(Self {
                state: Arc::new(RwLock::new(KvmState {
                    vm: None,
                    vm_state: VmState::Created,
                })),
                kvm: Arc::new(kvm),
            })
        }

        #[cfg(not(all(target_os = "linux", feature = "kvm")))]
        {
            Err(Error::Vmm(
                "KVM backend requires Linux + 'kvm' feature. \
                 Build with: cargo build --features kvm".into(),
            ))
        }
    }
}

// =============================================
// Hypervisor trait 实现
// =============================================

#[async_trait]
impl Hypervisor for KvmBackend {
    fn probe() -> ProbeResult {
        #[cfg(all(target_os = "linux", feature = "kvm"))]
        {
            // 1. 检查 /dev/kvm 设备
            match std::fs::metadata("/dev/kvm") {
                Err(_) => return ProbeResult::err("KVM", "/dev/kvm not found"),
                Ok(m) => {
                    use std::os::unix::fs::PermissionsExt;
                    let mode = m.permissions().mode();
                    if mode & 0o060 == 0 {
                        return ProbeResult::err("KVM", "/dev/kvm not readable/writable");
                    }
                }
            }
            // 2. 尝试打开并获取 API 版本
            match kvm_ioctls::Kvm::new() {
                Ok(kvm) => {
                    let ver = kvm.get_api_version();
                    tracing::debug!("KVM API version: {}", ver);
                    ProbeResult::ok("KVM")
                }
                Err(e) => ProbeResult::err("KVM", "failed to open /dev/kvm"),
            }
        }

        #[cfg(not(all(target_os = "linux", feature = "kvm")))]
        {
            #[cfg(target_os = "linux")]
            {
                // 即使不启用 kvm feature，也可以看 /dev/kvm 是否存在
                if std::path::Path::new("/dev/kvm").exists() {
                    ProbeResult::err("KVM", "feature 'kvm' not enabled - build with --features kvm")
                } else {
                    ProbeResult::err("KVM", "/dev/kvm not found")
                }
            }
            #[cfg(not(target_os = "linux"))]
            {
                ProbeResult::err("KVM", "not running on Linux")
            }
        }
    }

    fn backend_name(&self) -> &'static str {
        "KVM"
    }

    async fn create_vm(&self, config: &VmConfig) -> Result<()> {
        #[cfg(all(target_os = "linux", feature = "kvm"))]
        {
            let mut state = self.state.write();
            if state.vm.is_some() {
                return Err(Error::Vmm("VM already created".into()));
            }

            let vm = kvm_impl::KvmVm::create(&self.kvm, config)?;
            state.vm = Some(vm);
            state.vm_state = VmState::Created;
            Ok(())
        }

        #[cfg(not(all(target_os = "linux", feature = "kvm")))]
        Err(Error::Vmm("KVM not available".into()))
    }

    async fn start(&self) -> Result<()> {
        #[cfg(all(target_os = "linux", feature = "kvm"))]
        {
            let mut state = self.state.write();
            if state.vm.is_none() {
                return Err(Error::Vmm("VM not created".into()));
            }
            state.vm_state = VmState::Running;
            tracing::info!("KVM VM started");
            Ok(())
        }

        #[cfg(not(all(target_os = "linux", feature = "kvm")))]
        Err(Error::Vmm("KVM not available".into()))
    }

    async fn stop(&self) -> Result<()> {
        #[cfg(all(target_os = "linux", feature = "kvm"))]
        {
            let mut state = self.state.write();
            state.vm_state = VmState::Stopping;
            tracing::info!("KVM VM stop requested");
            Ok(())
        }

        #[cfg(not(all(target_os = "linux", feature = "kvm")))]
        Err(Error::Vmm("KVM not available".into()))
    }

    async fn run(&self) -> Result<i32> {
        #[cfg(all(target_os = "linux", feature = "kvm"))]
        {
            let state_arc = Arc::clone(&self.state);

            // 在专用线程上运行 vCPU（避免阻塞 Tokio executor）
            let result = tokio::task::spawn_blocking(move || -> Result<i32> {
                let mut state = state_arc.write();

                let vm = state.vm.as_mut().ok_or_else(|| Error::Vmm("VM not created".into()))?;

                // 默认从 0x8000 开始执行（内核入口简化地址）
                let entry_rip: u64 = 0x8000;
                vm.create_vcpus(entry_rip)?;

                tracing::info!("KVM run loop starting, entry RIP={:#x}", entry_rip);
                let exit_code = vm.run_vcpu0()?;

                state.vm_state = VmState::Stopped;
                tracing::info!("KVM VM exited with code {}", exit_code);
                Ok(exit_code)
            })
            .await
            .map_err(|e| Error::Vmm(format!("vCPU thread panicked: {e}")))?;

            self.state.write().vm_state = VmState::Stopped;
            result
        }

        #[cfg(not(all(target_os = "linux", feature = "kvm")))]
        Err(Error::Vmm("KVM not available".into()))
    }

    fn map_ram(&self, _hva: *mut u8, _gpa: u64, _size: u64) -> Result<()> {
        #[cfg(all(target_os = "linux", feature = "kvm"))]
        {
            // KVM: KVM_SET_USER_MEMORY_REGION 已由 KvmVm::create 在 slot 0 注册
            // 后续区域可以通过 VmFd::set_user_memory_region 添加
            Err(Error::Vmm(
                "KVM map_ram: use slot-based registration via KvmVm::create".into(),
            ))
        }
        #[cfg(not(all(target_os = "linux", feature = "kvm")))]
        Err(Error::Vmm("KVM not available".into()))
    }

    #[allow(clippy::too_many_arguments)]
    fn set_vcpu_entry(
        &self,
        _rip: u64, _rsp: u64, _rsi: u64,
        _cr0: u64, _cr3: u64, _cr4: u64, _efer: u64,
        _gdt_base: u64, _gdt_limit: u16,
    ) -> Result<()> {
        // KVM: 通过 VcpuFd::set_sregs / set_regs 设置，待实现
        Err(Error::Vmm("KVM set_vcpu_entry not yet implemented".into()))
    }
}
