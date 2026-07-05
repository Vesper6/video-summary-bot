//! Linux 内核与 Initramfs 加载器。
//!
//! VMM 启动时需要把客户机内核镜像、initramfs、rootfs 加载到客户机内存并设置启动参数。
//!
//! ## 启动流程（Linux x86_64）
//!
//! 1. 实模式：从 0x7c00 加载 MBR / 从 0x100000 加载 setup.bin
//! 2. 保护模式：从 0x100000 加载 vmlinux
//! 3. 跳到入口点（entry point）

use std::path::Path;

use crate::error::Result;
use crate::vmm::memory::GuestMemory;

/// 内核加载器。
pub struct KernelLoader;

impl KernelLoader {
    /// 把内核镜像加载到客户机内存。
    ///
    /// 默认加载到 0x100000（1 MB 处），这是 Linux 内核期望的传统位置。
    pub fn load_kernel(&self, mem: &GuestMemory, kernel_path: &Path) -> Result<u64> {
        let bytes = std::fs::read(kernel_path)?;
        tracing::info!(
            "loading kernel: {} bytes from {}",
            bytes.len(),
            kernel_path.display()
        );

        // 简化：直接把内核写到 0x100000
        // 真实实现需要解析 ELF 头，按 program header 加载
        let ram = mem.ram_regions();
        if let Some(ram) = ram.first() {
            ram.write(0x10_0000, &bytes)?;
        }

        // 返回入口点（简化：固定为 0x100000）
        Ok(0x10_0000)
    }

    /// 加载 initramfs。
    pub fn load_initramfs(&self, mem: &GuestMemory, initramfs_path: &Path) -> Result<()> {
        let bytes = std::fs::read(initramfs_path)?;
        tracing::info!(
            "loading initramfs: {} bytes from {}",
            bytes.len(),
            initramfs_path.display()
        );

        // 加载到内核之后的内存（简化）
        let ram = mem.ram_regions();
        if let Some(ram) = ram.first() {
            ram.write(0x20_0000, &bytes)?;
        }
        Ok(())
    }
}