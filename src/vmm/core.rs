//! VMM 核心：VM 生命周期 + 设备总线 + 启动协议。
//!
//! 参考 tenbox src/core/vmm/vm.cpp：
//! - 内存分配 → WHVP/KVM 映射
//! - bzImage 加载 → boot_params 填写 → GDT 写入
//! - VirtIO MMIO 总线初始化
//! - vCPU 线程启动 + MMIO exit 分发

use std::sync::Arc;
use parking_lot::RwLock;

use crate::devices::{VirtioMmioBus, VirtioConsole};
use crate::error::{Error, Result};
use crate::ipc::serial_path;
use crate::vmm::{VmConfig, VmState};
use crate::vmm::loader::{KernelLoader, InitialRegs, write_gdt};
use crate::vmm::memory::GuestRam;

/// VMM 实例。
///
/// 每个 Vmm 对应一个 VM，持有：
/// - VmConfig（不可变）
/// - GuestRam（已分配的客户机内存）
/// - VirtioMmioBus（所有 VirtIO 设备）
/// - VmState（可变）
pub struct Vmm {
    pub config: VmConfig,
    state: RwLock<VmState>,
    /// 客户机 RAM（页对齐，已映射）
    pub ram: Arc<GuestRam>,
    /// VirtIO MMIO 总线
    pub mmio_bus: Arc<parking_lot::Mutex<VirtioMmioBus>>,
    /// VirtIO Console（与 Guest Agent 通信）
    pub console: Arc<VirtioConsole>,
    /// Guest Agent channel：宿主读取 Guest 输出
    pub guest_output_rx: Arc<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<Vec<u8>>>>,
    /// Guest Agent channel：宿主写入 Guest 输入
    pub host_input_tx: tokio::sync::mpsc::Sender<Vec<u8>>,
}

impl Vmm {
    /// 创建 VMM：分配内存、初始化 VirtIO 总线。
    pub fn new(config: VmConfig) -> Result<Self> {
        let ram_bytes = config.memory_mb as usize * 1024 * 1024;
        let ram = GuestRam::allocate(0, ram_bytes)
            .map_err(|e| Error::Vmm(format!("RAM alloc failed: {e}")))?;
        let (console, guest_output_rx, host_input_tx) = VirtioConsole::new();

        // MMIO 总线：注册 Console
        let mut bus = VirtioMmioBus::new();
        let console_addr = bus.add_device(Arc::clone(&console) as Arc<dyn crate::devices::VirtioDevice>);
        tracing::info!("VirtIO Console MMIO base: {:#x}", console_addr);

        Ok(Self {
            config,
            state: RwLock::new(VmState::Created),
            ram: ram,
            mmio_bus: Arc::new(parking_lot::Mutex::new(bus)),
            console,
            guest_output_rx: Arc::new(tokio::sync::Mutex::new(guest_output_rx)),
            host_input_tx,
        })
    }

    /// 加载内核 + initrd，写入 boot_params 和 GDT。
    pub fn load_kernel(&self) -> Result<InitialRegs> {
        let kernel = self.config.kernel.as_ref()
            .ok_or_else(|| Error::Vmm("kernel path not set".into()))?;

        let initrd = self.config.initramfs.as_deref();
        let cmdline = self.config.cmdline.as_deref()
            .unwrap_or("console=hvc0 root=/dev/vda rw init=/sbin/init");

        let result = KernelLoader::load(
            &self.ram,
            kernel,
            initrd,
            cmdline,
            self.config.memory_mb,
        )?;

        // 写 GDT
        write_gdt(&self.ram)?;

        let regs = InitialRegs::for_linux_64(
            result.kernel_entry,
            result.boot_params_addr,
            self.config.memory_mb,
        );

        tracing::info!(
            "VM '{}' loaded: kernel={:#x} boot_params={:#x} initrd={:?}",
            self.config.name,
            result.kernel_entry,
            result.boot_params_addr,
            result.initrd_addr,
        );

        Ok(regs)
    }

    pub fn state(&self) -> VmState {
        *self.state.read()
    }

    pub fn set_state(&self, s: VmState) {
        *self.state.write() = s;
    }

    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// 处理 MMIO exit（从 WHVP/KVM run loop 调用）。
    pub fn handle_mmio_read(&self, gpa: u64, size: u8) -> Option<u32> {
        self.mmio_bus.lock().read(gpa, size)
    }

    pub fn handle_mmio_write(&self, gpa: u64, value: u32, size: u8) -> bool {
        self.mmio_bus.lock().write(gpa, value, size)
    }
}
