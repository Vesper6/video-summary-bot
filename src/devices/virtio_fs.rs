//! virtio-fs 共享文件系统。

use std::path::PathBuf;

use crate::devices::VirtioDeviceType;

/// virtio-fs 共享目录。
pub struct VirtioFs {
    /// 共享目录在 guest 内的挂载标签
    pub tag: String,
    /// 宿主机路径
    pub host_path: PathBuf,
    /// 是否只读
    pub read_only: bool,
}

impl VirtioFs {
    pub fn new(tag: impl Into<String>, host_path: PathBuf) -> Self {
        Self {
            tag: tag.into(),
            host_path,
            read_only: false,
        }
    }

    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    pub fn device_type(&self) -> VirtioDeviceType {
        VirtioDeviceType::Fs
    }
}