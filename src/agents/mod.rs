//! AI Agent 模块。
//!
//! - [`claude`] — Claude Code 集成

pub mod claude;

/// Agent 类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentKind {
    /// Claude Code
    Claude,
}

/// Agent 状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentState {
    /// 空闲
    Idle,
    /// 运行中
    Running,
    /// 完成
    Done,
    /// 失败
    Failed,
}
