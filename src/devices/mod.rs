//! virtio 设备模型。
//!
//! - [`virtio_mmio`]    — VirtIO MMIO 传输层 + 总线 ✅
//! - [`virtio_console`] — VirtIO Console（宿主↔Guest 串口通信） ✅
//! - [`virtio_block`]   — virtio-blk（块设备） 🚧
//! - [`virtio_net`]     — virtio-net（NAT 网络） 🚧
//! - [`virtio_fs`]      — virtiofs（共享目录） 🚧
//! - [`virtio_gpu`]     — virtio-gpu 🚧
//! - [`virtio_snd`]     — virtio-snd 🚧
//! - [`virtio_input`]   — virtio-input 🚧
//! - [`serial`]         — 16550A 串口（非 VirtIO） 🚧

pub mod serial;
pub mod virtio_block;
pub mod virtio_console;
pub mod virtio_fs;
pub mod virtio_gpu;
pub mod virtio_input;
pub mod virtio_mmio;
pub mod virtio_net;
pub mod virtio_snd;

pub use virtio_mmio::{VirtioMmioBus, VirtioMmio, VirtioDevice, VirtioDeviceId};
pub use virtio_console::VirtioConsole;

/// 旧版设备类型枚举（骨架文件兼容用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum VirtioDeviceType {
    /// virtio-net 网络设备
    Net     = 1,
    /// virtio-blk 块设备
    Block   = 2,
    /// virtio-console 控制台
    Console = 3,
    /// virtio-rng 随机数发生器
    Rng     = 4,
    /// virtio-balloon 内存气球
    Balloon = 5,
    /// virtio-scsi 存储控制器
    Scsi    = 8,
    /// virtio-gpu 图形设备
    Gpu     = 16,
    /// virtio-clock 时钟设备
    Clock   = 17,
    /// virtio-input 输入设备
    Input   = 18,
    /// virtio-snd 音频设备
    Sound   = 25,
    /// virtio-fs 共享文件系统
    Fs      = 26,
    /// virtio-rpmsg 远程处理器消息
    Rpmsg   = 35,
}
