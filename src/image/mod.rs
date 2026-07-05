//! 磁盘镜像格式。
//!
//! 支持：
//! - [`qcow2`] — QEMU Copy-On-Write v2
//! - [`raw`] — 原始格式

pub mod qcow2;
pub mod raw;

use std::path::Path;

use crate::error::Result;

/// 镜像格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    /// 原始格式
    Raw,
    /// qcow2 格式
    Qcow2,
}

impl ImageFormat {
    pub fn from_path(path: &Path) -> Self {
        match path.extension().and_then(|e| e.to_str()) {
            Some("qcow2") => ImageFormat::Qcow2,
            _ => ImageFormat::Raw,
        }
    }
}

/// 磁盘镜像抽象。
pub trait DiskImage: Send + Sync {
    /// 镜像格式。
    fn format(&self) -> ImageFormat;

    /// 镜像容量（字节）。
    fn capacity(&self) -> u64;

    /// 读取数据。
    fn read(&self, offset: u64, buf: &mut [u8]) -> Result<()>;

    /// 写入数据。
    fn write(&self, offset: u64, buf: &[u8]) -> Result<()>;

    /// 同步到磁盘。
    fn flush(&self) -> Result<()>;
}

/// 创建指定格式的镜像。
pub fn create_image(path: &Path, format: ImageFormat, size_bytes: u64) -> Result<Box<dyn DiskImage>> {
    match format {
        ImageFormat::Raw => raw::RawImage::create(path, size_bytes).map(|i| Box::new(i) as _),
        ImageFormat::Qcow2 => qcow2::Qcow2Image::create(path, size_bytes).map(|i| Box::new(i) as _),
    }
}

/// 打开已有镜像。
pub fn open_image(path: &Path) -> Result<Box<dyn DiskImage>> {
    match ImageFormat::from_path(path) {
        ImageFormat::Raw => raw::RawImage::open(path).map(|i| Box::new(i) as _),
        ImageFormat::Qcow2 => qcow2::Qcow2Image::open(path).map(|i| Box::new(i) as _),
    }
}