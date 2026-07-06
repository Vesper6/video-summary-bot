//! VirtIO MMIO 传输层。
//!
//! 参考 tenbox src/core/device/virtio/ 和 VirtIO 1.2 规范 §4.2。
//!
//! ## VirtIO MMIO 地址布局
//!
//! 每个设备占 0x200 字节 MMIO 空间：
//!
//! ```text
//! 0x000 MagicValue      R   0x74726976 ("virt")
//! 0x004 Version         R   2 (VirtIO 1.0+)
//! 0x008 DeviceID        R   设备类型 (1=net, 2=blk, 3=console, ...)
//! 0x00C VendorID        R   0x554D4551 ("QEMU")
//! 0x010 DeviceFeatures  R   设备支持的 feature bits
//! 0x014 DeviceFeatSel   W   选择哪个 32-bit feature word
//! 0x020 DriverFeatures  W   driver 确认的 feature bits
//! 0x024 DriverFeatSel   W
//! 0x030 QueueSel        W   选择 virtqueue 编号
//! 0x034 QueueNumMax     R   最大队列深度
//! 0x038 QueueNum        W   driver 设置的队列深度
//! 0x044 QueueReady      RW  队列就绪标志
//! 0x050 QueueNotify     W   通知设备（doorbell）
//! 0x060 InterruptStatus R   中断原因
//! 0x064 InterruptACK    W   中断确认
//! 0x070 Status          RW  设备状态字节
//! 0x080 QueueDescLow    W   Descriptor Table 低 32 位
//! 0x084 QueueDescHigh   W   Descriptor Table 高 32 位
//! 0x090 QueueDriverLow  W   Available Ring 低 32 位
//! 0x094 QueueDriverHigh W   Available Ring 高 32 位
//! 0x0A0 QueueDeviceLow  W   Used Ring 低 32 位
//! 0x0A4 QueueDeviceHigh W   Used Ring 高 32 位
//! 0x0FC ConfigGeneration R  配置代数
//! 0x100 Config[0..0xFF] RW  设备专用配置空间
//! ```

use std::sync::Arc;
use parking_lot::Mutex;

use crate::error::{Error, Result};

// =============================================
// VirtIO MMIO 寄存器偏移
// =============================================

pub const VIRTIO_MMIO_MAGIC_VALUE:      u64 = 0x000;
pub const VIRTIO_MMIO_VERSION:          u64 = 0x004;
pub const VIRTIO_MMIO_DEVICE_ID:        u64 = 0x008;
pub const VIRTIO_MMIO_VENDOR_ID:        u64 = 0x00C;
pub const VIRTIO_MMIO_DEVICE_FEATURES:  u64 = 0x010;
pub const VIRTIO_MMIO_DEVICE_FEAT_SEL:  u64 = 0x014;
pub const VIRTIO_MMIO_DRIVER_FEATURES:  u64 = 0x020;
pub const VIRTIO_MMIO_DRIVER_FEAT_SEL:  u64 = 0x024;
pub const VIRTIO_MMIO_QUEUE_SEL:        u64 = 0x030;
pub const VIRTIO_MMIO_QUEUE_NUM_MAX:    u64 = 0x034;
pub const VIRTIO_MMIO_QUEUE_NUM:        u64 = 0x038;
pub const VIRTIO_MMIO_QUEUE_READY:      u64 = 0x044;
pub const VIRTIO_MMIO_QUEUE_NOTIFY:     u64 = 0x050;
pub const VIRTIO_MMIO_INTERRUPT_STATUS: u64 = 0x060;
pub const VIRTIO_MMIO_INTERRUPT_ACK:    u64 = 0x064;
pub const VIRTIO_MMIO_STATUS:           u64 = 0x070;
pub const VIRTIO_MMIO_QUEUE_DESC_LOW:   u64 = 0x080;
pub const VIRTIO_MMIO_QUEUE_DESC_HIGH:  u64 = 0x084;
pub const VIRTIO_MMIO_QUEUE_DRIVER_LOW: u64 = 0x090;
pub const VIRTIO_MMIO_QUEUE_DRIVER_HIGH:u64 = 0x094;
pub const VIRTIO_MMIO_QUEUE_DEVICE_LOW: u64 = 0x0A0;
pub const VIRTIO_MMIO_QUEUE_DEVICE_HIGH:u64 = 0x0A4;
pub const VIRTIO_MMIO_CONFIG_GENERATION:u64 = 0x0FC;
pub const VIRTIO_MMIO_CONFIG:           u64 = 0x100;
pub const VIRTIO_MMIO_SIZE:             u64 = 0x200;

// =============================================
// VirtIO 设备 ID
// =============================================

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioDeviceId {
    Net     = 1,
    Block   = 2,
    Console = 3,
    Rng     = 4,
    Gpu     = 16,
    Input   = 18,
    Fs      = 26,
}

// =============================================
// VirtIO 状态位
// =============================================

pub const VIRTIO_STATUS_ACKNOWLEDGE:       u32 = 1;
pub const VIRTIO_STATUS_DRIVER:            u32 = 2;
pub const VIRTIO_STATUS_DRIVER_OK:         u32 = 4;
pub const VIRTIO_STATUS_FEATURES_OK:       u32 = 8;
pub const VIRTIO_STATUS_DEVICE_NEEDS_RESET:u32 = 64;
pub const VIRTIO_STATUS_FAILED:            u32 = 128;

// =============================================
// Virtqueue 描述符表（VirtIO 1.2 §2.7）
// =============================================

/// Descriptor 标志
pub const VIRTQ_DESC_F_NEXT:     u16 = 1;
pub const VIRTQ_DESC_F_WRITE:    u16 = 2;
pub const VIRTQ_DESC_F_INDIRECT: u16 = 4;

/// VirtIO 描述符（16 字节）
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct VirtqDesc {
    pub addr:  u64,   // Guest 物理地址
    pub len:   u32,
    pub flags: u16,
    pub next:  u16,   // 下一个描述符索引（NEXT 标志有效时）
}

/// Virtqueue 状态
#[derive(Debug, Default)]
pub struct Virtqueue {
    /// 队列深度
    pub size: u16,
    /// Descriptor Table GPA
    pub desc_addr:   u64,
    /// Available Ring GPA
    pub avail_addr:  u64,
    /// Used Ring GPA
    pub used_addr:   u64,
    /// 就绪标志
    pub ready: bool,
    /// 上次处理的 available index
    pub last_avail_idx: u16,
}

// =============================================
// VirtIO MMIO 设备（宿主侧）
// =============================================

/// VirtIO MMIO 设备 trait（每种设备实现）。
pub trait VirtioDevice: Send + Sync {
    fn device_id(&self) -> VirtioDeviceId;
    fn device_features(&self) -> u64;
    /// 处理 Virtqueue 通知（doorbell，queue_idx 指定哪个队列）。
    fn notify(&self, queue_idx: u32, queues: &[Virtqueue]) -> Result<()>;
    /// 读取设备配置空间（偏移 offset，大小 size）。
    fn read_config(&self, offset: u64, size: u8) -> u32 { 0 }
    /// 写设备配置空间。
    fn write_config(&self, offset: u64, value: u32, size: u8) {}
}

/// VirtIO MMIO 传输层（每个设备一个实例）。
pub struct VirtioMmio {
    /// MMIO 基址（GPA）
    pub base_addr: u64,
    /// 设备实现
    device: Arc<dyn VirtioDevice>,
    /// 内部状态
    state: Mutex<VirtioMmioState>,
}

#[derive(Default)]
struct VirtioMmioState {
    status:          u32,
    device_feat_sel: u32,
    driver_feat_sel: u32,
    driver_features: [u32; 2],
    queue_sel:       u32,
    queues:          Vec<Virtqueue>,
    interrupt_status:u32,
    config_gen:      u32,
}

impl VirtioMmio {
    const MAGIC:   u32 = 0x7472_6976; // "virt"
    const VERSION: u32 = 2;
    const VENDOR:  u32 = 0x5545_4D51; // "QEMU"

    pub fn new(base_addr: u64, device: Arc<dyn VirtioDevice>) -> Self {
        let num_queues = 4; // 默认预分配 4 个队列
        Self {
            base_addr,
            device,
            state: Mutex::new(VirtioMmioState {
                queues: (0..num_queues).map(|_| Virtqueue::default()).collect(),
                ..Default::default()
            }),
        }
    }

    /// 处理 Guest 的 MMIO 读（exit reason: MemoryAccess read）。
    pub fn mmio_read(&self, offset: u64, size: u8) -> u32 {
        let state = self.state.lock();

        match offset {
            VIRTIO_MMIO_MAGIC_VALUE      => Self::MAGIC,
            VIRTIO_MMIO_VERSION          => Self::VERSION,
            VIRTIO_MMIO_DEVICE_ID        => self.device.device_id() as u32,
            VIRTIO_MMIO_VENDOR_ID        => Self::VENDOR,

            VIRTIO_MMIO_DEVICE_FEATURES  => {
                let feat = self.device.device_features();
                if state.device_feat_sel == 0 { feat as u32 }
                else { (feat >> 32) as u32 }
            }

            VIRTIO_MMIO_QUEUE_NUM_MAX    => {
                let q = state.queue_sel as usize;
                if q < state.queues.len() { 256 } else { 0 }
            }

            VIRTIO_MMIO_QUEUE_READY      => {
                let q = state.queue_sel as usize;
                if q < state.queues.len() && state.queues[q].ready { 1 } else { 0 }
            }

            VIRTIO_MMIO_INTERRUPT_STATUS => state.interrupt_status,
            VIRTIO_MMIO_STATUS           => state.status,
            VIRTIO_MMIO_CONFIG_GENERATION=> state.config_gen,

            o if o >= VIRTIO_MMIO_CONFIG && o < VIRTIO_MMIO_CONFIG + 0x100 => {
                self.device.read_config(o - VIRTIO_MMIO_CONFIG, size)
            }

            _ => {
                tracing::trace!("virtio mmio read unknown offset={:#x}", offset);
                0
            }
        }
    }

    /// 处理 Guest 的 MMIO 写。
    pub fn mmio_write(&self, offset: u64, value: u32, size: u8) {
        let mut state = self.state.lock();

        match offset {
            VIRTIO_MMIO_DEVICE_FEAT_SEL  => state.device_feat_sel = value,
            VIRTIO_MMIO_DRIVER_FEAT_SEL  => state.driver_feat_sel = value,

            VIRTIO_MMIO_DRIVER_FEATURES  => {
                let sel = state.driver_feat_sel as usize;
                if sel < 2 { state.driver_features[sel] = value; }
            }

            VIRTIO_MMIO_QUEUE_SEL        => state.queue_sel = value,

            VIRTIO_MMIO_QUEUE_NUM        => {
                let q = state.queue_sel as usize;
                if q < state.queues.len() {
                    state.queues[q].size = value as u16;
                }
            }

            VIRTIO_MMIO_QUEUE_READY      => {
                let q = state.queue_sel as usize;
                if q < state.queues.len() {
                    state.queues[q].ready = value != 0;
                    tracing::debug!("virtio queue {} ready={}", q, value != 0);
                }
            }

            VIRTIO_MMIO_QUEUE_NOTIFY     => {
                let queues: Vec<_> = state.queues.iter().map(|q| Virtqueue {
                    size: q.size,
                    desc_addr: q.desc_addr,
                    avail_addr: q.avail_addr,
                    used_addr: q.used_addr,
                    ready: q.ready,
                    last_avail_idx: q.last_avail_idx,
                }).collect();
                drop(state);
                if let Err(e) = self.device.notify(value, &queues) {
                    tracing::error!("virtio notify error: {e}");
                }
                return;
            }

            VIRTIO_MMIO_INTERRUPT_ACK    => {
                state.interrupt_status &= !value;
            }

            VIRTIO_MMIO_STATUS           => {
                state.status = value;
                if value == 0 {
                    tracing::info!("virtio device reset (device_id={})",
                        self.device.device_id() as u32);
                    state.config_gen = state.config_gen.wrapping_add(1);
                }
                tracing::debug!("virtio status={:#x}", value);
            }

            VIRTIO_MMIO_QUEUE_DESC_LOW   => {
                let q = state.queue_sel as usize;
                if q < state.queues.len() {
                    state.queues[q].desc_addr =
                        (state.queues[q].desc_addr & 0xFFFF_FFFF_0000_0000) | value as u64;
                }
            }
            VIRTIO_MMIO_QUEUE_DESC_HIGH  => {
                let q = state.queue_sel as usize;
                if q < state.queues.len() {
                    state.queues[q].desc_addr =
                        (state.queues[q].desc_addr & 0x0000_0000_FFFF_FFFF) | ((value as u64) << 32);
                }
            }
            VIRTIO_MMIO_QUEUE_DRIVER_LOW => {
                let q = state.queue_sel as usize;
                if q < state.queues.len() {
                    state.queues[q].avail_addr =
                        (state.queues[q].avail_addr & 0xFFFF_FFFF_0000_0000) | value as u64;
                }
            }
            VIRTIO_MMIO_QUEUE_DRIVER_HIGH=> {
                let q = state.queue_sel as usize;
                if q < state.queues.len() {
                    state.queues[q].avail_addr =
                        (state.queues[q].avail_addr & 0x0000_0000_FFFF_FFFF) | ((value as u64) << 32);
                }
            }
            VIRTIO_MMIO_QUEUE_DEVICE_LOW => {
                let q = state.queue_sel as usize;
                if q < state.queues.len() {
                    state.queues[q].used_addr =
                        (state.queues[q].used_addr & 0xFFFF_FFFF_0000_0000) | value as u64;
                }
            }
            VIRTIO_MMIO_QUEUE_DEVICE_HIGH=> {
                let q = state.queue_sel as usize;
                if q < state.queues.len() {
                    state.queues[q].used_addr =
                        (state.queues[q].used_addr & 0x0000_0000_FFFF_FFFF) | ((value as u64) << 32);
                }
            }

            o if o >= VIRTIO_MMIO_CONFIG && o < VIRTIO_MMIO_CONFIG + 0x100 => {
                drop(state);
                self.device.write_config(o - VIRTIO_MMIO_CONFIG, value, size);
                return;
            }

            _ => {
                tracing::trace!("virtio mmio write unknown offset={:#x} val={:#x}", offset, value);
            }
        }
    }

    /// 触发设备中断（由设备实现调用，通知 Guest 有数据）。
    pub fn raise_interrupt(&self, reason: u32) {
        self.state.lock().interrupt_status |= reason;
        // 实际中断注入由 hypervisor 后端处理（WHVP/KVM IRQ 注入）
        tracing::debug!("virtio interrupt raised reason={:#x}", reason);
    }
}

// =============================================
// MMIO 总线（所有设备统一管理）
// =============================================

/// VirtIO MMIO 总线（管理多个设备的 MMIO 分发）。
pub struct VirtioMmioBus {
    devices: Vec<VirtioMmio>,
}

impl VirtioMmioBus {
    /// MMIO 区域基址（参考 tenbox / QEMU virt machine）
    pub const BASE_ADDR: u64 = 0xA000_0000;
    /// 每个设备占 0x200 字节
    pub const DEVICE_SIZE: u64 = VIRTIO_MMIO_SIZE;

    pub fn new() -> Self {
        Self { devices: Vec::new() }
    }

    /// 添加设备，自动分配 MMIO 地址。
    pub fn add_device(&mut self, device: Arc<dyn VirtioDevice>) -> u64 {
        let idx = self.devices.len() as u64;
        let base = Self::BASE_ADDR + idx * Self::DEVICE_SIZE;
        tracing::info!(
            "virtio device {:?} → MMIO {:#x}..{:#x}",
            device.device_id(),
            base,
            base + Self::DEVICE_SIZE
        );
        self.devices.push(VirtioMmio::new(base, device));
        base
    }

    /// 处理 MMIO 读。
    pub fn read(&self, gpa: u64, size: u8) -> Option<u32> {
        for dev in &self.devices {
            if gpa >= dev.base_addr && gpa < dev.base_addr + VIRTIO_MMIO_SIZE {
                return Some(dev.mmio_read(gpa - dev.base_addr, size));
            }
        }
        None
    }

    /// 处理 MMIO 写。
    pub fn write(&self, gpa: u64, value: u32, size: u8) -> bool {
        for dev in &self.devices {
            if gpa >= dev.base_addr && gpa < dev.base_addr + VIRTIO_MMIO_SIZE {
                dev.mmio_write(gpa - dev.base_addr, value, size);
                return true;
            }
        }
        false
    }

    /// 检查 GPA 是否在 VirtIO MMIO 区域内。
    pub fn contains(&self, gpa: u64) -> bool {
        !self.devices.is_empty() &&
            gpa >= Self::BASE_ADDR &&
            gpa < Self::BASE_ADDR + self.devices.len() as u64 * Self::DEVICE_SIZE
    }

    pub fn devices(&self) -> &[VirtioMmio] {
        &self.devices
    }
}

impl Default for VirtioMmioBus {
    fn default() -> Self {
        Self::new()
    }
}
