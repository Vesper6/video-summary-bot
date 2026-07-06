//! Linux x86/x86_64 启动协议（bzImage loader）。
//!
//! 参考 tenbox src/core/vmm/vm.cpp 的启动流程，以及 Linux 内核文档：
//! Documentation/x86/boot.rst
//!
//! ## 内存布局（tenbox 兼容）
//!
//! ```text
//! GPA 0x00000000 - 0x0009FFFF  低 640KB RAM
//! GPA 0x000A0000 - 0x000FFFFF  VGA/BIOS ROM（保留）
//! GPA 0x00100000 - ...         内核加载区（1MB+）
//!
//! boot_params   @ 0x0000_7000   （Linux boot_params / zero page）
//! cmdline       @ 0x0002_0000   （内核命令行）
//! setup code    @ 0x0009_0000   （bzImage setup sector）
//! kernel (vmlinux.bin) @ 0x0100_0000  （1 MB，保护模式入口）
//! initrd        @ 内存顶端附近（由 boot_params.ramdisk_image 指定）
//! ```

use std::path::Path;

use crate::error::{Error, Result};
use crate::vmm::memory::GuestRam;

// =============================================
// 物理地址常量（参考 tenbox + Linux boot.rst）
// =============================================

/// boot_params（zero page）加载地址
pub const BOOT_PARAMS_ADDR: u64 = 0x0000_7000;
/// 内核命令行地址
pub const CMDLINE_ADDR: u64    = 0x0002_0000;
/// bzImage setup sector 地址
pub const SETUP_ADDR: u64      = 0x0009_0000;
/// 保护模式内核加载地址（1 MB）
pub const KERNEL_ADDR: u64     = 0x0010_0000;
/// initrd 默认加载地址（内存顶 - 32MB）
pub const INITRD_ADDR_HINT: u64 = 0x0400_0000; // 64MB 处，避让内核

/// 内核魔数（setup_header.boot_flag）
const BOOT_FLAG: u16 = 0xAA55;
/// setup_header.header 字段魔数
const HDR_MAGIC: u32 = 0x5372_6448; // "HdrS"

// =============================================
// boot_params / setup_header 结构（精简，仅必要字段）
// 完整定义见 linux/arch/x86/include/uapi/asm/bootparam.h
// =============================================

/// Linux x86 setup_header（偏移 0x1F1 处，共 0x7F 字节）。
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct SetupHeader {
    pub setup_sects:         u8,   // 0x1F1
    pub root_flags:          u16,  // 0x1F2
    pub syssize:             u32,  // 0x1F4
    pub ram_size:            u16,  // 0x1F8
    pub vid_mode:            u16,  // 0x1FA
    pub root_dev:            u16,  // 0x1FC
    pub boot_flag:           u16,  // 0x1FE  must be 0xAA55
    pub jump:                u16,  // 0x200
    pub header:              u32,  // 0x202  "HdrS"
    pub version:             u16,  // 0x206
    pub realmode_swtch:      u32,  // 0x208
    pub start_sys_seg:       u16,  // 0x20C
    pub kernel_version:      u16,  // 0x20E
    pub type_of_loader:      u8,   // 0x210
    pub loadflags:           u8,   // 0x211
    pub setup_move_size:     u16,  // 0x212
    pub code32_start:        u32,  // 0x214  32-bit 入口
    pub ramdisk_image:       u32,  // 0x218  initrd 加载地址
    pub ramdisk_size:        u32,  // 0x21C  initrd 大小
    pub bootsect_kludge:     u32,  // 0x220
    pub heap_end_ptr:        u16,  // 0x224
    pub ext_loader_ver:      u8,   // 0x226
    pub ext_loader_type:     u8,   // 0x227
    pub cmd_line_ptr:        u32,  // 0x228  cmdline 地址
    pub initrd_addr_max:     u32,  // 0x22C  initrd 最高地址
    pub kernel_alignment:    u32,  // 0x230
    pub relocatable_kernel:  u8,   // 0x234
    pub min_alignment:       u8,   // 0x235
    pub xloadflags:          u16,  // 0x236
    pub cmdline_size:        u32,  // 0x238
    pub hardware_subarch:    u32,  // 0x23C
    pub hardware_subarch_data: u64, // 0x240
    pub payload_offset:      u32,  // 0x248
    pub payload_length:      u32,  // 0x24C
    pub setup_data:          u64,  // 0x250
    pub pref_address:        u64,  // 0x258
    pub init_size:           u32,  // 0x260
    pub handover_offset:     u32,  // 0x264
}

/// E820 内存段类型
#[repr(u32)]
#[derive(Clone, Copy)]
pub enum E820Type {
    Ram      = 1,
    Reserved = 2,
    Acpi     = 3,
}

/// E820 内存段
#[repr(C, packed)]
#[derive(Clone, Copy, Default)]
pub struct E820Entry {
    pub addr:   u64,
    pub size:   u64,
    pub e820type: u32,
}

/// boot_params（zero page）— 精简版，只包含 VMM 需要填写的字段。
/// 完整结构 128 × 16 = 2048 字节（0x800）。
#[repr(C)]
pub struct BootParams {
    pub screen_info:       [u8; 0x40],  // 0x000
    pub apm_bios_info:     [u8; 0x14],  // 0x040
    _pad0:                 [u8; 0x0C],  // 0x054
    pub tboot_addr:        u64,         // 0x058 (low 8 bytes)
    _pad1:                 [u8; 0x08],  // 0x060
    pub ist_info:          [u8; 0x10],  // 0x068
    pub acpi_rsdp_addr:    u64,         // 0x070 (low 8 bytes)
    _pad2:                 [u8; 0x08],  // 0x078 (hi bytes)
    pub hd0_info:          [u8; 0x10],  // 0x080
    pub hd1_info:          [u8; 0x10],  // 0x090
    pub sys_desc_table:    [u8; 0x10],  // 0x0A0
    pub olpc_ofw_header:   [u8; 0x10],  // 0x0B0
    pub ext_ramdisk_image: u32,         // 0x0C0
    pub ext_ramdisk_size:  u32,         // 0x0C4
    pub ext_cmd_line_ptr:  u32,         // 0x0C8
    _pad3:                 [u8; 0x74],  // 0x0CC
    pub e820_table_count:  u8,          // 0x1E8 (e820_entries)
    pub eddbuf_entries:    u8,          // 0x1E9
    pub edd_mbr_sig_buf_entries: u8,    // 0x1EA
    pub kbd_status:        u8,          // 0x1EB
    pub secure_boot:       u8,          // 0x1EC
    _pad4:                 [u8; 0x02],  // 0x1ED
    pub sentinel:          u8,          // 0x1EF
    _pad5:                 u8,          // 0x1F0
    pub hdr:               SetupHeader, // 0x1F1
    _pad6:                 [u8; 0x24],  // after hdr (0x1F1+0x74=0x265, pad to 0x290)
    pub edd_mbr_sig_buffer:[u8; 0x40],  // 0x290
    pub e820_table:        [E820Entry; 128], // 0x2D0 (128 × 20 = 0xA00)
    _pad7:                 [u8; 0x30],  // 0xCD0
    pub eddbuf:            [u8; 0x1EC], // 0xD00
}

// SAFETY: BootParams 是纯 C 数据结构，不含引用
unsafe impl Send for BootParams {}
unsafe impl Sync for BootParams {}

// =============================================
// bzImage 解析
// =============================================

/// bzImage 解析结果。
pub struct BzImageInfo {
    /// setup_header（从文件偏移 0x1F1 读取）
    pub hdr: SetupHeader,
    /// setup sectors 大小（字节）
    pub setup_size: usize,
    /// 保护模式内核（vmlinux.bin）数据
    pub kernel_data: Vec<u8>,
}

/// 解析 bzImage 文件，返回 setup_header + 保护模式内核数据。
pub fn parse_bzimage(bytes: &[u8]) -> Result<BzImageInfo> {
    if bytes.len() < 0x300 {
        return Err(Error::Vmm("bzImage too small".into()));
    }

    // 读取 setup_header（偏移 0x1F1）—— 使用 read_unaligned 避免 packed 对齐 UB
    let hdr: SetupHeader = unsafe {
        std::ptr::read_unaligned(bytes.as_ptr().add(0x1F1) as *const SetupHeader)
    };

    // packed 结构字段不能取引用，用字节偏移 + read_unaligned 安全读取
    // boot_flag: linux/arch/x86/boot/header.S @ offset 0x1FE → struct offset 0x0D
    // header:    "HdrS" magic @ 0x202 → struct offset 0x11
    let boot_flag: u16 = unsafe {
        std::ptr::read_unaligned(bytes.as_ptr().add(0x1FE) as *const u16)
    };
    let header: u32 = unsafe {
        std::ptr::read_unaligned(bytes.as_ptr().add(0x202) as *const u32)
    };
    let setup_sects_raw: u8 = bytes[0x1F1]; // setup_sects is the very first byte

    if boot_flag != BOOT_FLAG {
        return Err(Error::Vmm(format!(
            "invalid boot_flag: {:#x} (expected {:#x})",
            boot_flag, BOOT_FLAG
        )));
    }
    if header != HDR_MAGIC {
        return Err(Error::Vmm("not a bzImage (missing HdrS magic)".into()));
    }

    let setup_sects = if setup_sects_raw == 0 { 4 } else { setup_sects_raw as usize };
    let setup_size  = (setup_sects + 1) * 512;

    if bytes.len() <= setup_size {
        return Err(Error::Vmm("bzImage: no kernel data after setup".into()));
    }

    let kernel_data = bytes[setup_size..].to_vec();

    tracing::info!(
        "bzImage: setup_sects={} setup_size={}B kernel_data={}KB",
        setup_sects,
        setup_size,
        kernel_data.len() / 1024
    );

    Ok(BzImageInfo { hdr, setup_size, kernel_data })
}

// =============================================
// 内核加载器（主入口）
// =============================================

/// 内核加载结果。
pub struct LoadResult {
    /// 内核保护模式入口 GPA
    pub kernel_entry: u64,
    /// boot_params 地址
    pub boot_params_addr: u64,
    /// initrd 加载地址（如果有）
    pub initrd_addr: Option<u64>,
    /// initrd 大小
    pub initrd_size: u32,
}

/// Linux bzImage + initramfs 加载器。
pub struct KernelLoader;

impl KernelLoader {
    /// 完整加载流程：
    /// 1. 解析 bzImage → setup_header + kernel_data
    /// 2. 把 setup sector 写到 SETUP_ADDR（0x9000）
    /// 3. 把 kernel_data 写到 KERNEL_ADDR（0x100000）
    /// 4. 加载 initrd（如果有）到 INITRD_ADDR_HINT
    /// 5. 填写 boot_params（zero page）并写入 BOOT_PARAMS_ADDR
    pub fn load(
        ram:          &GuestRam,
        kernel_path:  &Path,
        initrd_path:  Option<&Path>,
        cmdline:      &str,
        ram_size_mb:  u32,
    ) -> Result<LoadResult> {
        // ── 1. 读取并解析 bzImage ────────────────
        let kernel_bytes = std::fs::read(kernel_path)
            .map_err(|e| Error::Vmm(format!("read kernel {:?}: {e}", kernel_path)))?;

        let bzimg = parse_bzimage(&kernel_bytes)?;

        // ── 2. 写 setup sector ───────────────────
        ram.write(SETUP_ADDR, &kernel_bytes[..bzimg.setup_size])?;
        tracing::debug!("setup sector → GPA {:#x}", SETUP_ADDR);

        // ── 3. 写保护模式内核 ────────────────────
        ram.write(KERNEL_ADDR, &bzimg.kernel_data)?;
        tracing::info!(
            "kernel → GPA {:#x} ({} KB)",
            KERNEL_ADDR,
            bzimg.kernel_data.len() / 1024
        );

        // ── 4. 加载 initrd ───────────────────────
        let (initrd_addr, initrd_size) = if let Some(initrd) = initrd_path {
            let initrd_bytes = std::fs::read(initrd)
                .map_err(|e| Error::Vmm(format!("read initrd {:?}: {e}", initrd)))?;
            let size = initrd_bytes.len() as u32;

            // initrd 放在内核之后，按 4MB 对齐
            let addr = align_up(KERNEL_ADDR + bzimg.kernel_data.len() as u64, 0x40_0000);
            ram.write(addr, &initrd_bytes)?;
            tracing::info!("initrd → GPA {:#x} ({} KB)", addr, size / 1024);
            (Some(addr), size)
        } else {
            (None, 0)
        };

        // ── 5. 写命令行 ──────────────────────────
        let cmdline_bytes = {
            let mut v = cmdline.as_bytes().to_vec();
            v.push(0); // NUL 终止
            v
        };
        ram.write(CMDLINE_ADDR, &cmdline_bytes)?;
        tracing::debug!("cmdline → GPA {:#x}: {:?}", CMDLINE_ADDR, cmdline);

        // ── 6. 填写 boot_params ──────────────────
        let bp = build_boot_params(
            &bzimg.hdr,
            ram_size_mb,
            initrd_addr,
            initrd_size,
        );

        // boot_params 是 2KB 结构，逐字段写（避免 packed 对齐问题）
        let bp_bytes = unsafe {
            std::slice::from_raw_parts(
                &bp as *const BootParams as *const u8,
                std::mem::size_of::<BootParams>(),
            )
        };
        // BootParams 可能 > 2KB，只写前 2KB（e820_table 起点后）
        let write_size = bp_bytes.len().min(0x1000);
        ram.write(BOOT_PARAMS_ADDR, &bp_bytes[..write_size])?;
        tracing::debug!("boot_params → GPA {:#x} ({} bytes)", BOOT_PARAMS_ADDR, write_size);

        let kernel_entry = KERNEL_ADDR;
        tracing::info!("kernel entry: GPA {:#x}", kernel_entry);

        Ok(LoadResult {
            kernel_entry,
            boot_params_addr: BOOT_PARAMS_ADDR,
            initrd_addr,
            initrd_size,
        })
    }

    /// 向后兼容的旧接口（骨架用）。
    pub fn load_kernel(&self, _mem: &crate::vmm::memory::GuestMemory, kernel_path: &Path) -> Result<u64> {
        tracing::warn!("load_kernel: use KernelLoader::load() for full boot protocol support");
        let bytes = std::fs::read(kernel_path)?;
        tracing::info!("kernel: {} bytes", bytes.len());
        Ok(KERNEL_ADDR)
    }

    pub fn load_initramfs(&self, _mem: &crate::vmm::memory::GuestMemory, initramfs_path: &Path) -> Result<()> {
        tracing::warn!("load_initramfs: use KernelLoader::load() for full boot protocol support");
        let bytes = std::fs::read(initramfs_path)?;
        tracing::info!("initrd: {} bytes", bytes.len());
        Ok(())
    }
}

// =============================================
// boot_params 构建
// =============================================

fn build_boot_params(
    hdr:          &SetupHeader,
    ram_size_mb:  u32,
    initrd_addr:  Option<u64>,
    initrd_size:  u32,
) -> BootParams {
    let ram_size_bytes = ram_size_mb as u64 * 1024 * 1024;

    // 复制 setup_header，填写 VMM 字段
    let mut new_hdr = *hdr;
    new_hdr.type_of_loader = 0xFF;         // 未知 bootloader
    new_hdr.loadflags     |= 0x01;         // LOADED_HIGH: kernel at 1MB+
    new_hdr.cmd_line_ptr   = CMDLINE_ADDR as u32;
    new_hdr.heap_end_ptr   = 0xFE00;       // setup heap end
    new_hdr.loadflags     |= 0x80;         // CAN_USE_HEAP

    if let Some(addr) = initrd_addr {
        new_hdr.ramdisk_image = addr as u32;
        new_hdr.ramdisk_size  = initrd_size;
    }

    // 创建 zero page（全零初始化）
    // SAFETY: BootParams 是 POD，零初始化合法
    let mut bp: BootParams = unsafe { std::mem::zeroed() };
    bp.hdr = new_hdr;

    // 填写 e820 内存图（参考 tenbox vm.cpp）
    let mut e820_count = 0u8;

    // 低 640KB RAM (0x0000_0000 - 0x0009_FFFF)
    bp.e820_table[0] = E820Entry {
        addr:     0x0000_0000,
        size:     0x0009_F000,
        e820type: E820Type::Ram as u32,
    };
    e820_count += 1;

    // ISA 保留区域 (0x000A_0000 - 0x000F_FFFF)
    bp.e820_table[1] = E820Entry {
        addr:     0x000A_0000,
        size:     0x0006_0000,
        e820type: E820Type::Reserved as u32,
    };
    e820_count += 1;

    // 主 RAM (1MB - 顶端)
    let main_ram_base: u64 = 0x0010_0000;
    let main_ram_size: u64 = ram_size_bytes.saturating_sub(main_ram_base);
    if main_ram_size > 0 {
        bp.e820_table[2] = E820Entry {
            addr:     main_ram_base,
            size:     main_ram_size,
            e820type: E820Type::Ram as u32,
        };
        e820_count += 1;
    }

    bp.e820_table_count = e820_count;

    tracing::debug!(
        "e820: {} entries, RAM={} MB",
        e820_count,
        ram_size_bytes / 1024 / 1024
    );

    bp
}

// =============================================
// 初始 vCPU 寄存器（x86 保护模式）
// =============================================

/// 初始 vCPU 寄存器值（参考 tenbox whvp_vm.cpp set_initial_regs）。
pub struct InitialRegs {
    pub rip:    u64,  /// 内核入口
    pub rsp:    u64,  /// 初始栈（kernel top - 64KB）
    pub rsi:    u64,  /// boot_params 地址（Linux 启动约定）
    pub rflags: u64,
    pub cr0:    u64,  /// PE=1（保护模式）
    pub cr3:    u64,  /// 页表（先置 0，内核自己设置）
    pub cr4:    u64,
    pub efer:   u64,  // Long mode enable (IA32_EFER.LME)
}

impl InitialRegs {
    /// 为 64-bit Linux 启动生成初始寄存器值。
    pub fn for_linux_64(
        kernel_entry:    u64,
        boot_params_gpa: u64,
        ram_size_mb:     u32,
    ) -> Self {
        let ram_top = (ram_size_mb as u64) * 1024 * 1024;
        Self {
            rip:    kernel_entry,
            rsi:    boot_params_gpa,           // Linux boot 约定：RSI = boot_params
            rsp:    ram_top.saturating_sub(0x1_0000), // 初始栈
            rflags: 0x0000_0002,               // 保留位
            cr0:    0x0000_0011,               // PE=1, ET=1（保护模式，不开分页）
            cr3:    0,
            cr4:    0x0000_0020,               // PAE=1
            efer:   0x0000_0100,               // LME=1（Long Mode Enable）
        }
    }
}

// =============================================
// GDT 初始化（tenbox 兼容）
// =============================================

/// GDT 段描述符（64-bit flat）。
const GDT_NULL:   u64 = 0x0000_0000_0000_0000;
const GDT_CODE64: u64 = 0x00AF_9A00_0000_FFFF; // 64-bit code, DPL0, present
const GDT_DATA64: u64 = 0x00CF_9200_0000_FFFF; // 64-bit data, DPL0, present

/// GDT 加载地址（参考 tenbox）
pub const GDT_ADDR: u64 = 0x0000_5000;

/// 把 GDT 写入客户机内存，返回 GDT base + limit。
pub fn write_gdt(ram: &GuestRam) -> Result<(u64, u16)> {
    let gdt: [u64; 3] = [GDT_NULL, GDT_CODE64, GDT_DATA64];
    let gdt_bytes = unsafe {
        std::slice::from_raw_parts(
            gdt.as_ptr() as *const u8,
            gdt.len() * 8,
        )
    };
    ram.write(GDT_ADDR, gdt_bytes)?;
    let limit = (gdt.len() * 8 - 1) as u16;
    tracing::debug!("GDT written at GPA {:#x}, limit={:#x}", GDT_ADDR, limit);
    Ok((GDT_ADDR, limit))
}

// =============================================
// 辅助函数
// =============================================

fn align_up(addr: u64, align: u64) -> u64 {
    (addr + align - 1) & !(align - 1)
}

// =============================================
// 单元测试
// =============================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bzimage_invalid() {
        let result = parse_bzimage(&[0u8; 100]);
        assert!(result.is_err());
    }

    #[test]
    fn test_align_up() {
        assert_eq!(align_up(0x100001, 0x40_0000), 0x40_0000);
        assert_eq!(align_up(0x40_0000, 0x40_0000), 0x40_0000);
        assert_eq!(align_up(0x40_0001, 0x40_0000), 0x80_0000);
    }

    #[test]
    fn test_boot_params_size() {
        // BootParams 应该 <= 4096 字节
        assert!(std::mem::size_of::<BootParams>() <= 4096);
    }
}
