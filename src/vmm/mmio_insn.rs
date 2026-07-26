//! MMIO 访问指令解码（x86-64 mov 子集）。
//!
//! WHVP 的 MemoryAccess exit 不会自动模拟指令——VMM 必须自己解码
//! 触发 exit 的指令、执行设备读写、写回寄存器并推进 RIP。
//!
//! Linux 驱动的 MMIO 访问（`readb/w/l/q`、`writeb/w/l/q`）编译后
//! 都是简单的 `mov` / `movzx` 寻址形式，本模块只解码这个子集：
//!
//! | 指令               | 编码        | 说明                     |
//! |--------------------|-------------|--------------------------|
//! | `mov r/m, r`       | 88/89 /r    | store 8/16/32/64         |
//! | `mov r, r/m`       | 8A/8B /r    | load 8/16/32/64          |
//! | `movzx r, r/m8`    | 0F B6 /r    | load 8（零扩展）         |
//! | `movzx r, r/m16`   | 0F B7 /r    | load 16（零扩展）        |
//! | `mov r/m, imm`     | C6/C7 /0    | store 立即数             |

/// 解码出的 MMIO 操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmioOp {
    /// 内存 → 寄存器（reg 为 x86 GPR 编号 0-15，0=RAX…7=RDI, 8-15=R8-R15）
    Load {
        /// 目标寄存器编号
        reg: u8,
        /// true：结果零扩展写满 64 位（movzx / 32 位 mov / 64 位 mov）；
        /// false：仅覆盖低 8/16 位，高位保留（8/16 位普通 mov）
        zero_extend: bool,
    },
    /// 寄存器 → 内存
    Store {
        /// 源寄存器编号
        reg: u8,
    },
    /// 立即数 → 内存
    StoreImm {
        /// 立即数值（已按访问宽度截断）
        imm: u64,
    },
}

/// 解码结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MmioInsn {
    /// 操作类型
    pub op: MmioOp,
    /// 访问宽度（1/2/4/8 字节）
    pub access_size: u8,
    /// 指令总长度（用于推进 RIP）
    pub len: u8,
}

/// 从指令字节解码 MMIO 访问。返回 `None` 表示不认识（调用方记日志跳过）。
pub fn decode_mmio_insn(bytes: &[u8]) -> Option<MmioInsn> {
    let mut i = 0usize;
    let mut opsize16 = false;
    let mut rex: u8 = 0;

    // 前缀（最多扫 5 字节防御）
    while i < bytes.len().min(5) {
        match bytes[i] {
            0x66 => {
                opsize16 = true;
                i += 1;
            }
            // 地址宽度 / lock / rep / 段前缀：对解码宽度无影响，跳过
            0x67 | 0xF0 | 0xF2 | 0xF3 | 0x2E | 0x36 | 0x3E | 0x26 | 0x64 | 0x65 => i += 1,
            0x40..=0x4F => {
                rex = bytes[i];
                i += 1;
                break; // REX 必须紧邻 opcode
            }
            _ => break,
        }
    }

    let rex_w = rex & 0x8 != 0;
    let rex_r = (rex >> 2) & 1;
    let opcode = *bytes.get(i)?;
    i += 1;

    // (op 构造闭包放后面，先取 modrm)
    let two_byte = opcode == 0x0F;
    let opcode2 = if two_byte {
        let b = *bytes.get(i)?;
        i += 1;
        Some(b)
    } else {
        None
    };

    let modrm = *bytes.get(i)?;
    i += 1;
    let modb = modrm >> 6;
    let reg = ((modrm >> 3) & 0x7) | (rex_r << 3);
    let rm = modrm & 0x7;

    if modb == 3 {
        return None; // 寄存器-寄存器，不是内存访问
    }

    // SIB
    let mut sib_base5 = false;
    if rm == 4 {
        let sib = *bytes.get(i)?;
        i += 1;
        sib_base5 = sib & 0x7 == 5;
    }
    // 位移
    i += match modb {
        1 => 1,
        2 => 4,
        0 if rm == 5 => 4,              // RIP-relative disp32
        0 if rm == 4 && sib_base5 => 4, // SIB base=101 + mod=00 → disp32
        _ => 0,
    };

    let wide_size = if rex_w {
        8
    } else if opsize16 {
        2
    } else {
        4
    };

    let (op, access_size) = match (opcode, opcode2) {
        (0x88, None) => (MmioOp::Store { reg }, 1),
        (0x89, None) => (MmioOp::Store { reg }, wide_size),
        (0x8A, None) => (
            MmioOp::Load {
                reg,
                zero_extend: false,
            },
            1,
        ),
        (0x8B, None) => (
            MmioOp::Load {
                reg,
                zero_extend: wide_size >= 4,
            },
            wide_size,
        ),
        (0x0F, Some(0xB6)) => (
            MmioOp::Load {
                reg,
                zero_extend: true,
            },
            1,
        ),
        (0x0F, Some(0xB7)) => (
            MmioOp::Load {
                reg,
                zero_extend: true,
            },
            2,
        ),
        (0xC6, None) => {
            let imm = *bytes.get(i)? as u64;
            i += 1;
            (MmioOp::StoreImm { imm }, 1)
        }
        (0xC7, None) => {
            if opsize16 {
                let b = bytes.get(i..i + 2)?;
                i += 2;
                (
                    MmioOp::StoreImm {
                        imm: u16::from_le_bytes(b.try_into().ok()?) as u64,
                    },
                    2,
                )
            } else {
                let b = bytes.get(i..i + 4)?;
                i += 4;
                let imm32 = u32::from_le_bytes(b.try_into().ok()?);
                // REX.W C7：imm32 符号扩展到 64 位
                let imm = if rex_w {
                    imm32 as i32 as i64 as u64
                } else {
                    imm32 as u64
                };
                (MmioOp::StoreImm { imm }, if rex_w { 8 } else { 4 })
            }
        }
        _ => return None,
    };

    Some(MmioInsn {
        op,
        access_size,
        len: i as u8,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mov_load_32() {
        // mov (%rdi), %eax
        let insn = decode_mmio_insn(&[0x8B, 0x07]).unwrap();
        assert_eq!(
            insn,
            MmioInsn {
                op: MmioOp::Load {
                    reg: 0,
                    zero_extend: true
                },
                access_size: 4,
                len: 2
            }
        );
    }

    #[test]
    fn mov_store_32() {
        // mov %edx, (%rax)
        let insn = decode_mmio_insn(&[0x89, 0x10]).unwrap();
        assert_eq!(
            insn,
            MmioInsn {
                op: MmioOp::Store { reg: 2 },
                access_size: 4,
                len: 2
            }
        );
    }

    #[test]
    fn mov_load_64() {
        // mov (%rdi), %rax
        let insn = decode_mmio_insn(&[0x48, 0x8B, 0x07]).unwrap();
        assert_eq!(insn.access_size, 8);
        assert_eq!(insn.len, 3);
        assert_eq!(
            insn.op,
            MmioOp::Load {
                reg: 0,
                zero_extend: true
            }
        );
    }

    #[test]
    fn movzx_load_8() {
        // movzbl (%rdi), %eax
        let insn = decode_mmio_insn(&[0x0F, 0xB6, 0x07]).unwrap();
        assert_eq!(insn.access_size, 1);
        assert_eq!(insn.len, 3);
        assert_eq!(
            insn.op,
            MmioOp::Load {
                reg: 0,
                zero_extend: true
            }
        );
    }

    #[test]
    fn movzx_load_16_with_rex_r() {
        // movzwl (%rsi), %r10d → 44 0f b7 16
        let insn = decode_mmio_insn(&[0x44, 0x0F, 0xB7, 0x16]).unwrap();
        assert_eq!(insn.access_size, 2);
        assert_eq!(insn.len, 4);
        assert_eq!(
            insn.op,
            MmioOp::Load {
                reg: 10,
                zero_extend: true
            }
        );
    }

    #[test]
    fn mov_load_16_merges() {
        // mov (%rdi), %ax → 66 8b 07：只覆盖低 16 位
        let insn = decode_mmio_insn(&[0x66, 0x8B, 0x07]).unwrap();
        assert_eq!(insn.access_size, 2);
        assert_eq!(
            insn.op,
            MmioOp::Load {
                reg: 0,
                zero_extend: false
            }
        );
    }

    #[test]
    fn mov_store_8() {
        // mov %cl, (%rdx) → 88 0a
        let insn = decode_mmio_insn(&[0x88, 0x0A]).unwrap();
        assert_eq!(insn.access_size, 1);
        assert_eq!(insn.op, MmioOp::Store { reg: 1 });
    }

    #[test]
    fn sib_addressing() {
        // mov (%r12), %eax → 41 8b 04 24（rm=100 → SIB）
        let insn = decode_mmio_insn(&[0x41, 0x8B, 0x04, 0x24]).unwrap();
        assert_eq!(insn.len, 4);
        assert_eq!(
            insn.op,
            MmioOp::Load {
                reg: 0,
                zero_extend: true
            }
        );
    }

    #[test]
    fn disp8_and_disp32() {
        // mov 0x50(%rdi), %eax → 8b 47 50
        let insn = decode_mmio_insn(&[0x8B, 0x47, 0x50]).unwrap();
        assert_eq!(insn.len, 3);
        // mov 0x12345678(%rdi), %eax → 8b 87 78 56 34 12
        let insn = decode_mmio_insn(&[0x8B, 0x87, 0x78, 0x56, 0x34, 0x12]).unwrap();
        assert_eq!(insn.len, 6);
    }

    #[test]
    fn store_imm32() {
        // movl $0x1, (%rax) → c7 00 01 00 00 00
        let insn = decode_mmio_insn(&[0xC7, 0x00, 0x01, 0x00, 0x00, 0x00]).unwrap();
        assert_eq!(insn.len, 6);
        assert_eq!(insn.access_size, 4);
        assert_eq!(insn.op, MmioOp::StoreImm { imm: 1 });
    }

    #[test]
    fn store_imm8() {
        // movb $0xAB, (%rdi) → c6 07 ab
        let insn = decode_mmio_insn(&[0xC6, 0x07, 0xAB]).unwrap();
        assert_eq!(insn.len, 3);
        assert_eq!(insn.op, MmioOp::StoreImm { imm: 0xAB });
    }

    #[test]
    fn register_register_not_mmio() {
        // mov %eax, %edx（mod=11）不是内存访问
        assert!(decode_mmio_insn(&[0x89, 0xC2]).is_none());
    }

    #[test]
    fn unknown_opcode_rejected() {
        assert!(decode_mmio_insn(&[0x0F, 0xAE, 0x38]).is_none()); // clflush
        assert!(decode_mmio_insn(&[]).is_none());
    }
}
