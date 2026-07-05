//! virtio 设备模型。
//!
//! 实现 virtio 协议（MMIO 总线）的设备后端：
//! - [`virtio_block`] — virtio-blk（块设备）
//! - [`virtio_net`] — virtio-net（网络）
//! - [`virtio_fs`] — virtio-fs（共享文件系统）
//! - [`virtio_gpu`] — virtio-gpu（图形，SPICE）
//! - [`virtio_snd`] — virtio-snd（音频）
//! - [`virtio_input`] — virtio-input（键鼠）
//! - [`serial`] — 串口控制台

pub mod serial;
pub mod virtio_block;
pub mod virtio_fs;
pub mod virtio_gpu;
pub mod virtio_input;
pub mod virtio_net;
pub mod virtio_snd;

/// virtio 设备 ID。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum VirtioDeviceType {
    /// 网络
    Net = 1,
    /// 块设备
    Block = 2,
    /// 控制台
    Console = 3,
    /// 熵源
    Rng = 4,
    /// 内存气球
    Balloon = 5,
    /// SCSI
    Scsi = 8,
    /// GPU
    Gpu = 16,
    /// 时钟
    Clock = 17,
    /// 输入
    Input = 18,
    /// 声音
    Sound = 25,
    /// 文件系统
    Fs = 26,
    /// 共享内存
    Rpmsg = 35,
}