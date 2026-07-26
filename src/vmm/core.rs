//! VMM 核心：VM 生命周期 + 设备总线 + 启动协议。
//!
//! 参考 tenbox src/core/vmm/vm.cpp：
//! - 内存分配 → WHVP/KVM 映射
//! - bzImage 加载 → boot_params 填写 → GDT 写入
//! - VirtIO MMIO 总线初始化
//! - vCPU 线程启动 + MMIO exit 分发

use parking_lot::{Mutex, RwLock};
use std::sync::Arc;

use crate::devices::{Pit, VirtioBlock, VirtioConsole, VirtioMmioBus};
use crate::error::{Error, Result};
use crate::vmm::loader::{write_gdt, InitialRegs, KernelLoader};
use crate::vmm::memory::GuestRam;
use crate::vmm::{VmConfig, VmState};

/// VMM 实例。
///
/// 每个 Vmm 对应一个 VM，持有：
/// - VmConfig（不可变）
/// - GuestRam（已分配的客户机内存）
/// - VirtioMmioBus（所有 VirtIO 设备）
/// - VmState（可变）
pub struct Vmm {
    /// VM 配置（不可变）
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
    /// 8254 PIT（TSC / LAPIC 定时器校准）
    pub pit: Mutex<Pit>,
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
        let console_addr =
            bus.add_device(Arc::clone(&console) as Arc<dyn crate::devices::VirtioDevice>);
        tracing::info!("VirtIO Console MMIO base: {:#x}", console_addr);

        // virtio-blk：rootfs 镜像存在时注册（root=/dev/vda 的后端）
        if let Some(rootfs) = config.rootfs.clone().filter(|p| p.exists()) {
            match VirtioBlock::open(rootfs.clone(), Arc::clone(&ram), false) {
                Ok(blk) => {
                    let addr = bus.add_device(blk as Arc<dyn crate::devices::VirtioDevice>);
                    tracing::info!(
                        "VirtIO Block MMIO base: {:#x} (image: {})",
                        addr,
                        rootfs.display()
                    );
                }
                Err(e) => tracing::warn!("virtio-blk disabled: {e}"),
            }
        }

        Ok(Self {
            config,
            state: RwLock::new(VmState::Created),
            ram,
            mmio_bus: Arc::new(parking_lot::Mutex::new(bus)),
            console,
            guest_output_rx: Arc::new(tokio::sync::Mutex::new(guest_output_rx)),
            host_input_tx,
            pit: Mutex::new(Pit::new()),
        })
    }

    /// 加载内核 + initrd，写入 boot_params 和 GDT。
    pub fn load_kernel(&self) -> Result<InitialRegs> {
        let kernel = self
            .config
            .kernel
            .as_ref()
            .ok_or_else(|| Error::Vmm("kernel path not set".into()))?;

        let initrd = self.config.initramfs.as_deref();
        let mut cmdline = self
            .config
            .cmdline
            .as_deref()
            .unwrap_or("console=hvc0 root=/dev/vda rw init=/sbin/init")
            .to_string();

        // 追加 virtio_mmio.device= 参数（无设备树/ACPI，内核靠 cmdline 发现设备）
        let virtio_params = self.mmio_bus.lock().kernel_cmdline_params();
        if !virtio_params.is_empty() && !cmdline.contains("virtio_mmio.device=") {
            cmdline.push(' ');
            cmdline.push_str(&virtio_params);
        }
        tracing::info!("kernel cmdline: {cmdline}");

        let result =
            KernelLoader::load(&self.ram, kernel, initrd, &cmdline, self.config.memory_mb)?;

        // 写 GDT + identity-map 页表（long mode 必需）
        write_gdt(&self.ram)?;
        crate::vmm::loader::write_page_tables(&self.ram)?;

        // 使用 64-bit 入口（保护模式内核代码 + 0x200）
        let entry_64 = crate::vmm::loader::KERNEL_ENTRY_64;
        let regs =
            InitialRegs::for_linux_64(entry_64, result.boot_params_addr, self.config.memory_mb);

        tracing::info!(
            "VM '{}' loaded: kernel_entry={:#x} boot_params={:#x} initrd={:?}",
            self.config.name,
            entry_64,
            result.boot_params_addr,
            result.initrd_addr,
        );

        Ok(regs)
    }

    /// 当前 VM 状态。
    pub fn state(&self) -> VmState {
        *self.state.read()
    }

    /// 设置 VM 状态。
    pub fn set_state(&self, s: VmState) {
        *self.state.write() = s;
    }

    /// VM 名称。
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// 处理 MMIO exit（从 WHVP/KVM run loop 调用）。
    pub fn handle_mmio_read(&self, gpa: u64, size: u8) -> Option<u32> {
        self.mmio_bus.lock().read(gpa, size)
    }

    /// 处理 MMIO 写 exit（从 WHVP/KVM run loop 调用）。
    pub fn handle_mmio_write(&self, gpa: u64, value: u32, size: u8) -> bool {
        self.mmio_bus.lock().write(gpa, value, size)
    }
}

impl crate::hypervisor::VmIoHandler for Vmm {
    fn pio_read(&self, port: u16, _size: u8) -> Option<u32> {
        if Pit::handles_port(port) {
            Some(self.pit.lock().io_read(port) as u32)
        } else {
            None
        }
    }

    fn pio_write(&self, port: u16, value: u32, _size: u8) -> bool {
        if Pit::handles_port(port) {
            self.pit.lock().io_write(port, value as u8);
            true
        } else {
            false
        }
    }

    fn mmio_read(&self, gpa: u64, size: u8) -> Option<u64> {
        let bus = self.mmio_bus.lock();
        if size == 8 {
            // 64 位访问拆成两次 32 位（VirtIO MMIO 寄存器都是 32 位）
            let lo = bus.read(gpa, 4)? as u64;
            let hi = bus.read(gpa + 4, 4).unwrap_or(0) as u64;
            Some(lo | (hi << 32))
        } else {
            bus.read(gpa, size).map(|v| v as u64)
        }
    }

    fn mmio_write(&self, gpa: u64, value: u64, size: u8) -> bool {
        let bus = self.mmio_bus.lock();
        if size == 8 {
            let lo = bus.write(gpa, value as u32, 4);
            let hi = bus.write(gpa + 4, (value >> 32) as u32, 4);
            lo || hi
        } else {
            bus.write(gpa, value as u32, size)
        }
    }

    fn take_pending_irq(&self) -> Option<u8> {
        self.mmio_bus.lock().take_pending_irq()
    }
}
