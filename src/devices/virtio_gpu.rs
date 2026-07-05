//! virtio-gpu 图形设备 + SPICE 协议。

use crate::devices::VirtioDeviceType;

/// virtio-gpu 设备。
pub struct VirtioGpu {
    /// 显示器宽度（像素）
    pub width: u32,
    /// 显示器高度（像素）
    pub height: u32,
    /// 是否启用 SPICE 输出
    pub enable_spice: bool,
}

impl VirtioGpu {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            enable_spice: true,
        }
    }

    pub fn device_type(&self) -> VirtioDeviceType {
        VirtioDeviceType::Gpu
    }
}