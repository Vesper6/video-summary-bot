//! virtio-net 网络设备。

use crate::devices::VirtioDeviceType;

/// virtio-net 设备。
pub struct VirtioNet {
    /// 后端 tap 设备名（Linux）/ 接口 GUID（Windows）
    pub backend: String,
    /// 客户机 MAC 地址
    pub mac: [u8; 6],
}

impl VirtioNet {
    pub fn new(backend: impl Into<String>, mac: [u8; 6]) -> Self {
        Self {
            backend: backend.into(),
            mac,
        }
    }

    pub fn device_type(&self) -> VirtioDeviceType {
        VirtioDeviceType::Net
    }
}