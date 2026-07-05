//! 原始格式磁盘镜像。
//!
//! 直接对应宿主文件，无压缩、无 copy-on-write。

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use parking_lot::Mutex;

use crate::error::Result;
use crate::image::{DiskImage, ImageFormat};

/// 原始格式镜像。
pub struct RawImage {
    file: Mutex<File>,
    capacity: u64,
}

impl RawImage {
    pub fn create(path: &Path, size_bytes: u64) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        file.set_len(size_bytes)?;
        Ok(Self {
            file: Mutex::new(file),
            capacity: size_bytes,
        })
    }

    pub fn open(path: &Path) -> Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        let capacity = file.metadata()?.len();
        Ok(Self {
            file: Mutex::new(file),
            capacity,
        })
    }
}

impl DiskImage for RawImage {
    fn format(&self) -> ImageFormat {
        ImageFormat::Raw
    }

    fn capacity(&self) -> u64 {
        self.capacity
    }

    fn read(&self, offset: u64, buf: &mut [u8]) -> Result<()> {
        let mut f = self.file.lock();
        f.seek(SeekFrom::Start(offset))?;
        f.read_exact(buf)?;
        Ok(())
    }

    fn write(&self, offset: u64, buf: &[u8]) -> Result<()> {
        let mut f = self.file.lock();
        f.seek(SeekFrom::Start(offset))?;
        f.write_all(buf)?;
        Ok(())
    }

    fn flush(&self) -> Result<()> {
        self.file.lock().flush()?;
        Ok(())
    }
}