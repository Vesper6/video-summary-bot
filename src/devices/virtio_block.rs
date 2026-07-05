//! virtio-blk 块设备模型。

use crate::devices::VirtioDeviceType;

/// virtio-blk 设备。
pub struct VirtioBlock {
    /// 后端镜像路径
    pub image_path: std::path::PathBuf,
    /// 容量（扇区数，每扇区 512 字节）
    pub capacity_sectors: u64,
}

impl VirtioBlock {
    pub fn new(image_path: std::path::PathBuf) -> Self {
        let capacity_sectors = std::fs::metadata(&image_path)
            .map(|m| m.len() / 512)
            .unwrap_or(0);
        Self {
            image_path,
            capacity_sectors,
        }
    }

    pub fn device_type(&self) -> VirtioDeviceType {
        VirtioDeviceType::Block
    }
}