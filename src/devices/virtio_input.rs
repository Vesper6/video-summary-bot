//! virtio-input 键鼠输入设备。

use crate::devices::VirtioDeviceType;

/// 输入设备类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    /// 键盘
    Keyboard,
    /// 鼠标
    Mouse,
    /// 触摸板
    Touchpad,
}

/// virtio-input 设备。
pub struct VirtioInput {
    pub kind: InputKind,
}

impl VirtioInput {
    pub fn new(kind: InputKind) -> Self {
        Self { kind }
    }

    pub fn device_type(&self) -> VirtioDeviceType {
        VirtioDeviceType::Input
    }
}