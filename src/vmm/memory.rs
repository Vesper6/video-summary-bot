//! 客户机物理内存管理。
//!
//! VMM 负责分配一块大块宿主内存（hva），把它映射到客户机物理地址空间（gpa）。
//! 客户机页表（gva → gpa）由 guest kernel 管理，VMM 不直接干预。

use std::sync::Arc;

use parking_lot::Mutex;

/// 客户机物理内存区域。
pub struct GuestRam {
    /// 起始 GPA（Guest Physical Address）
    base_gpa: u64,
    /// 大小（字节）
    size: usize,
    /// 映射的宿主虚拟地址
    hva_ptr: *mut u8,
    /// 是否映射
    mapped: bool,
}

// SAFETY: GuestRam 由 VMM 串行访问（单线程分配，多线程只读）
unsafe impl Send for GuestRam {}
unsafe impl Sync for GuestRam {}

impl GuestRam {
    /// 分配指定大小的客户机内存。
    pub fn allocate(base_gpa: u64, size: usize) -> Result<Arc<Self>, &'static str> {
        if size == 0 || size % 4096 != 0 {
            return Err("size must be non-zero and 4KB aligned");
        }

        let layout = std::alloc::Layout::from_size_align(size, 4096)
            .map_err(|_| "invalid memory layout")?;
        // SAFETY: layout 非零、按页对齐
        let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
        if ptr.is_null() {
            return Err("failed to allocate guest RAM");
        }

        Ok(Arc::new(Self {
            base_gpa,
            size,
            hva_ptr: ptr,
            mapped: false,
        }))
    }

    /// 客户机物理地址。
    pub fn base_gpa(&self) -> u64 {
        self.base_gpa
    }

    /// 内存大小（字节）。
    pub fn size(&self) -> usize {
        self.size
    }

    /// 客户机地址 → 宿主指针。
    ///
    /// # Safety
    ///
    /// 调用方必须保证 gpa 在此区域内。
    pub unsafe fn hva(&self, gpa: u64) -> Option<*mut u8> {
        if gpa < self.base_gpa || gpa >= self.base_gpa + self.size as u64 {
            return None;
        }
        Some(self.hva_ptr.add((gpa - self.base_gpa) as usize))
    }

    /// 读取客户机内存到 buffer。
    pub fn read(&self, gpa: u64, buf: &mut [u8]) -> crate::error::Result<()> {
        // SAFETY: 调用方需保证 gpa + buf.len() 在区域内
        unsafe {
            let src = self
                .hva(gpa)
                .ok_or_else(|| crate::error::Error::Vmm(format!("gpa {gpa:#x} out of range")))?;
            std::ptr::copy_nonoverlapping(src, buf.as_mut_ptr(), buf.len());
        }
        Ok(())
    }

    /// 从 buffer 写入客户机内存。
    pub fn write(&self, gpa: u64, buf: &[u8]) -> crate::error::Result<()> {
        // SAFETY: 调用方需保证 gpa + buf.len() 在区域内
        unsafe {
            let dst = self
                .hva(gpa)
                .ok_or_else(|| crate::error::Error::Vmm(format!("gpa {gpa:#x} out of range")))?;
            std::ptr::copy_nonoverlapping(buf.as_ptr(), dst, buf.len());
        }
        Ok(())
    }

    pub fn mapped(&self) -> bool {
        self.mapped
    }

    pub fn set_mapped(&mut self, mapped: bool) {
        self.mapped = mapped;
    }
}

impl Drop for GuestRam {
    fn drop(&mut self) {
        if !self.hva_ptr.is_null() && self.size > 0 {
            // SAFETY: 由 allocate 创建，layout 已知
            unsafe {
                std::alloc::dealloc(
                    self.hva_ptr,
                    std::alloc::Layout::from_size_align_unchecked(self.size, 4096),
                );
            }
        }
    }
}

/// 客户机内存总览（包含 RAM、BIOS、MMIO 等）。
pub struct GuestMemory {
    /// RAM 区域
    ram: Mutex<Vec<Arc<GuestRam>>>,
    /// 总 RAM 大小
    total_ram_size: usize,
}

impl GuestMemory {
    pub fn new() -> Self {
        Self {
            ram: Mutex::new(Vec::new()),
            total_ram_size: 0,
        }
    }

    pub fn add_ram(&mut self, region: Arc<GuestRam>) {
        self.total_ram_size += region.size();
        self.ram.lock().push(region);
    }

    /// 获取所有 RAM 区域（只读快照）。
    pub fn ram_regions(&self) -> Vec<Arc<GuestRam>> {
        self.ram.lock().clone()
    }

    pub fn total_ram_size(&self) -> usize {
        self.total_ram_size
    }
}

impl Default for GuestMemory {
    fn default() -> Self {
        Self::new()
    }
}