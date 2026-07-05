//! qcow2 格式磁盘镜像。
//!
//! QEMU Copy-On-Write v2：
//! - 支持压缩（zlib / zstd）
//! - 支持 copy-on-write（基于 backing file）
//! - 支持快照
//!
//! 当前为占位实现（接口已定义，逻辑待补充）。

use std::path::Path;

use crate::error::{Error, Result};
use crate::image::{DiskImage, ImageFormat};

/// qcow2 镜像（占位）。
pub struct Qcow2Image {
    path: std::path::PathBuf,
    capacity: u64,
}

impl Qcow2Image {
    pub fn create(_path: &Path, _size_bytes: u64) -> Result<Self> {
        // TODO: 写 qcow2 header + L1/L2 table
        Err(Error::Image("qcow2 create not implemented yet".into()))
    }

    pub fn open(path: &Path) -> Result<Self> {
        // TODO: 解析 qcow2 header
        Err(Error::Image(format!(
            "qcow2 open not implemented yet: {}",
            path.display()
        )))
    }
}

impl DiskImage for Qcow2Image {
    fn format(&self) -> ImageFormat {
        ImageFormat::Qcow2
    }

    fn capacity(&self) -> u64 {
        self.capacity
    }

    fn read(&self, _offset: u64, _buf: &mut [u8]) -> Result<()> {
        Err(Error::Image("qcow2 read not implemented yet".into()))
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> Result<()> {
        Err(Error::Image("qcow2 write not implemented yet".into()))
    }

    fn flush(&self) -> Result<()> {
        Ok(())
    }
}