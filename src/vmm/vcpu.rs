//! vCPU 抽象。

use serde::{Deserialize, Serialize};

/// 虚拟 CPU 状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VcpuState {
    /// vCPU ID（在 VM 内从 0 开始）
    pub id: u32,

    /// 是否在 HLT 状态
    pub halted: bool,

    /// 通用寄存器（x86_64: rax, rbx, rcx, rdx, rsi, rdi, rbp, r8-r15）
    #[serde(skip)]
    pub regs: Vec<u64>,

    /// RIP（指令指针）
    pub rip: u64,

    /// RSP（栈指针）
    pub rsp: u64,

    /// RFLAGS
    pub rflags: u64,
}

impl VcpuState {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            halted: false,
            regs: vec![0; 16],
            rip: 0,
            rsp: 0,
            rflags: 0x2, // 初始 RFLAGS：IF=0, 保留位 1
        }
    }
}

/// vCPU 退出原因（hypervisor 返回）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VcpuExit {
    /// 未知原因
    Unknown,
    /// 客户机请求停机
    Halt,
    /// 客户机请求关机
    Shutdown,
    /// I/O 端口读写
    Io { port: u16, direction: IoDirection, size: u8 },
    /// MMIO 读写
    Mmio { addr: u64, direction: IoDirection, size: u8 },
    /// 客户机故障
    FailEntry { code: u32 },
}

/// I/O 方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoDirection {
    /// 读取
    In,
    /// 写出
    Out,
}

/// vCPU 处理后的动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VcpuAction {
    /// 继续运行
    Continue,
    /// 重新调度
    Reschedule,
    /// 停止 VM
    Stop,
}