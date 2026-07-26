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
    /// 输入设备类型
    pub kind: InputKind,
}

impl VirtioInput {
    /// 创建指定类型的输入设备。
    pub fn new(kind: InputKind) -> Self {
        Self { kind }
    }

    /// 返回设备类型（Input）。
    pub fn device_type(&self) -> VirtioDeviceType {
        VirtioDeviceType::Input
    }
}
