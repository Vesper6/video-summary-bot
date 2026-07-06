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

/// 64-bit 入口偏移：bzImage 保护模式代码 + 0x200 是 64-bit 入口点
/// （见 Documentation/x86/boot.rst §64-bit BOOT PROTOCOL）
pub const KERNEL_ENTRY_64: u64 = KERNEL_ADDR + 0x200;

// 页表位置（identity-map 前 4GB，用 2MB 大页）
// 放在 0x30000 区域，避开 GDT(0x5000)/boot_params(0x7000)/cmdline(0x20000)/kernel(0x100000)
/// PML4（顶层页表）
pub const PML4_ADDR: u64 = 0x0003_0000;
/// PDPT（第 3 级，4 个有效项 → 4 个 PD）
pub const PDPT_ADDR: u64 = 0x0003_1000;
/// PD 起始地址（4 个 PD，每个 512 × 2MB = 1GB，共 4GB）
pub const PD_ADDR:   u64 = 0x0003_2000;
/// identity-map 覆盖的 GB 数
pub const IDENTITY_MAP_GB: u64 = 4;

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

        // ── 6. 填写 boot_params（原始字节方式，保证偏移正确）──
        let bp_bytes = build_boot_params_bytes(
            &kernel_bytes,
            bzimg.setup_size,
            ram_size_mb,
            initrd_addr,
            initrd_size,
        );
        ram.write(BOOT_PARAMS_ADDR, &bp_bytes)?;
        tracing::debug!("boot_params → GPA {:#x} ({} bytes)", BOOT_PARAMS_ADDR, bp_bytes.len());

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

/// zero-page 内 setup_header 的绝对偏移量（Linux boot protocol 固定值）。
mod zp {
    pub const HDR_START:        usize = 0x1F1; // setup_header 起点
    pub const TYPE_OF_LOADER:   usize = 0x210;
    pub const LOADFLAGS:        usize = 0x211;
    pub const RAMDISK_IMAGE:    usize = 0x218;
    pub const RAMDISK_SIZE:     usize = 0x21C;
    pub const HEAP_END_PTR:     usize = 0x224;
    pub const CMD_LINE_PTR:     usize = 0x228;
    pub const E820_ENTRIES:     usize = 0x1E8;
    pub const E820_TABLE:       usize = 0x2D0; // 每项 20 字节 (u64 addr, u64 size, u32 type)
}

/// 用原始字节构建 zero-page（4KB），保证所有字段落在正确的绝对偏移。
///
/// 做法（参考 firecracker / crosvm）：
/// 1. 从 bzImage 复制 setup_header 原始字节（file 0x1F1..setup_size 内）到 zp[0x1F1..]
/// 2. 按绝对偏移 patch VMM 需要设置的字段
/// 3. 写入 e820 表
fn build_boot_params_bytes(
    kernel_bytes: &[u8],
    setup_size:   usize,
    ram_size_mb:  u32,
    initrd_addr:  Option<u64>,
    initrd_size:  u32,
) -> Vec<u8> {
    let ram_size_bytes = ram_size_mb as u64 * 1024 * 1024;
    let mut zp = vec![0u8; 0x1000]; // 4KB zero page

    // 1. 复制 setup_header 原始字节：bzImage[0x1F1 .. min(0x268, setup_size)]
    //    setup_header 最长到约 0x268（含 handover_offset），足够覆盖 init_size(0x260)
    let hdr_end = 0x268.min(setup_size);
    if kernel_bytes.len() >= hdr_end && hdr_end > zp::HDR_START {
        zp[zp::HDR_START..hdr_end]
            .copy_from_slice(&kernel_bytes[zp::HDR_START..hdr_end]);
    }

    // 2. patch VMM 字段（绝对偏移）
    let put_u8  = |zp: &mut [u8], off: usize, v: u8|  zp[off] = v;
    let put_u16 = |zp: &mut [u8], off: usize, v: u16| zp[off..off+2].copy_from_slice(&v.to_le_bytes());
    let put_u32 = |zp: &mut [u8], off: usize, v: u32| zp[off..off+4].copy_from_slice(&v.to_le_bytes());

    put_u8(&mut zp, zp::TYPE_OF_LOADER, 0xFF); // 未知 bootloader

    // loadflags: |= LOADED_HIGH(0x01) | CAN_USE_HEAP(0x80)
    let loadflags = zp[zp::LOADFLAGS] | 0x01 | 0x80;
    put_u8(&mut zp, zp::LOADFLAGS, loadflags);

    put_u16(&mut zp, zp::HEAP_END_PTR, 0xFE00);
    put_u32(&mut zp, zp::CMD_LINE_PTR, CMDLINE_ADDR as u32);

    if let Some(addr) = initrd_addr {
        put_u32(&mut zp, zp::RAMDISK_IMAGE, addr as u32);
        put_u32(&mut zp, zp::RAMDISK_SIZE, initrd_size);
    }

    // 3. e820 内存图
    let mut entries: Vec<(u64, u64, u32)> = Vec::new();
    entries.push((0x0000_0000, 0x0009_F000, E820Type::Ram as u32));      // 低 640KB
    entries.push((0x000A_0000, 0x0006_0000, E820Type::Reserved as u32)); // ISA hole
    let main_base = 0x0010_0000u64;
    let main_size = ram_size_bytes.saturating_sub(main_base);
    if main_size > 0 {
        entries.push((main_base, main_size, E820Type::Ram as u32));      // 1MB+
    }

    put_u8(&mut zp, zp::E820_ENTRIES, entries.len() as u8);
    for (i, (addr, size, typ)) in entries.iter().enumerate() {
        let off = zp::E820_TABLE + i * 20;
        zp[off..off+8].copy_from_slice(&addr.to_le_bytes());
        zp[off+8..off+16].copy_from_slice(&size.to_le_bytes());
        zp[off+16..off+20].copy_from_slice(&typ.to_le_bytes());
    }

    tracing::debug!(
        "zero-page built: {} e820 entries, RAM={} MB, hdr copied [{:#x}..{:#x}]",
        entries.len(), ram_size_mb, zp::HDR_START, hdr_end
    );

    zp
}

// =============================================
// 初始 vCPU 寄存器（x86 保护模式）
// =============================================

/// 初始 vCPU 寄存器值（64-bit long mode，参考 firecracker / tenbox）。
pub struct InitialRegs {
    pub rip:    u64,  // 内核 64-bit 入口
    pub rsp:    u64,  // 初始栈
    pub rsi:    u64,  // boot_params 地址（Linux 启动约定）
    pub rflags: u64,
    pub cr0:    u64,  // PE=1 | PG=1（保护模式 + 分页）
    pub cr3:    u64,  // 指向 PML4
    pub cr4:    u64,  // PAE=1
    pub efer:   u64,  // LME=1 | LMA=1（long mode active）
}

impl InitialRegs {
    /// 为 64-bit Linux 启动生成初始寄存器值。
    ///
    /// 进入 long mode 需要：CR0.PE=1 & CR0.PG=1，CR4.PAE=1，
    /// EFER.LME=1（硬件在开分页时置 LMA），CR3 指向 identity-map 页表。
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
            rflags: 0x0000_0002,               // 保留位（bit1 恒为 1）
            cr0:    0x8000_0011,               // PG=1(bit31) | ET=1(bit4) | PE=1(bit0)
            cr3:    PML4_ADDR,                  // 指向 identity-map 页表
            cr4:    0x0000_0020,               // PAE=1(bit5)
            efer:   0x0000_0500,               // LMA=1(bit10) | LME=1(bit8)
        }
    }
}

// =============================================
// GDT 初始化（64-bit long mode，参考 tenbox / firecracker）
// =============================================

/// GDT 段描述符。布局遵循 Linux 64-bit boot protocol：
/// index 0=null, 1=null(unused), 2=__BOOT_CS(0x10), 3=__BOOT_DS(0x18)。
/// 见 Documentation/x86/boot.rst："GDT must have __BOOT_CS(0x10) and __BOOT_DS(0x18)"
const GDT_NULL:   u64 = 0x0000_0000_0000_0000;
/// 64-bit code：P=1 DPL=0 S=1 Type=Execute/Read, L=1（long mode）, G=1
const GDT_CODE64: u64 = 0x00AF_9B00_0000_FFFF;
/// 64-bit data：P=1 DPL=0 S=1 Type=Read/Write, G=1, D/B=1
const GDT_DATA64: u64 = 0x00CF_9300_0000_FFFF;

/// GDT 加载地址
pub const GDT_ADDR: u64 = 0x0000_5000;
/// __BOOT_CS 选择子（GDT index 2）
pub const SEL_CODE64: u16 = 0x10;
/// __BOOT_DS 选择子（GDT index 3）
pub const SEL_DATA64: u16 = 0x18;
/// GDT 项数
pub const GDT_ENTRIES: usize = 4;

/// 把 GDT 写入客户机内存，返回 GDT base + limit。
pub fn write_gdt(ram: &GuestRam) -> Result<(u64, u16)> {
    // index: 0=null 1=null 2=code(0x10) 3=data(0x18)
    let gdt: [u64; GDT_ENTRIES] = [GDT_NULL, GDT_NULL, GDT_CODE64, GDT_DATA64];
    let gdt_bytes = unsafe {
        std::slice::from_raw_parts(gdt.as_ptr() as *const u8, gdt.len() * 8)
    };
    ram.write(GDT_ADDR, gdt_bytes)?;
    let limit = (gdt.len() * 8 - 1) as u16;
    tracing::debug!("GDT written at GPA {:#x}, limit={:#x}", GDT_ADDR, limit);
    Ok((GDT_ADDR, limit))
}

/// 写入 identity-map 页表（前 4GB，2MB 大页），供 long mode 使用。
///
/// 布局：
/// - PML4[0] → PDPT
/// - PDPT[0..4] → PD0..PD3（每个覆盖 1GB）
/// - PDi[0..512] → 每个 2MB 大页
pub fn write_page_tables(ram: &GuestRam) -> Result<()> {
    // 页表项标志：P(present)=1, RW=1
    const PTE_P_RW: u64 = 0x3;
    // 2MB 大页额外需要 PS(page size)=1（bit 7）
    const PTE_PS: u64 = 0x80;

    // PML4[0] → PDPT
    ram.write(PML4_ADDR, &(PDPT_ADDR | PTE_P_RW).to_le_bytes())?;

    // PDPT[0..N] → PD0..PDN（每个 PD 覆盖 1GB）
    for g in 0..IDENTITY_MAP_GB {
        let pd_addr = PD_ADDR + g * 0x1000;
        ram.write(PDPT_ADDR + g * 8, &(pd_addr | PTE_P_RW).to_le_bytes())?;

        // 每个 PD 有 512 项，每项映射一个 2MB 大页
        for i in 0..512u64 {
            let phys = g * 0x4000_0000 + i * 0x20_0000; // 1GB*g + 2MB*i
            let entry = phys | PTE_P_RW | PTE_PS;
            ram.write(pd_addr + i * 8, &entry.to_le_bytes())?;
        }
    }

    tracing::debug!(
        "page tables written: PML4@{:#x} PDPT@{:#x} PD@{:#x} (identity-map 0..{}GB)",
        PML4_ADDR, PDPT_ADDR, PD_ADDR, IDENTITY_MAP_GB
    );
    Ok(())
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
