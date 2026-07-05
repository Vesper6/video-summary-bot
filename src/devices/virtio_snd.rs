//! virtio-snd 音频设备。

use crate::devices::VirtioDeviceType;

/// virtio-snd 设备。
pub struct VirtioSnd {
    /// 采样率（Hz）
    pub sample_rate: u32,
    /// 声道数
    pub channels: u8,
}

impl VirtioSnd {
    pub fn new() -> Self {
        Self {
            sample_rate: 48000,
            channels: 2,
        }
    }

    pub fn device_type(&self) -> VirtioDeviceType {
        VirtioDeviceType::Sound
    }
}

impl Default for VirtioSnd {
    fn default() -> Self {
        Self::new()
    }
}