//! 任务规划器。
//!
//! 使用 Claude 把用户任务分解为可执行步骤（pipeline）。

use serde::{Deserialize, Serialize};

/// 任务规划步骤。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    /// 步骤描述
    pub description: String,
    /// 步骤类型
    pub kind: StepKind,
}

/// 步骤类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    /// 启动 VM
    StartVm { name: String },
    /// 抓取评论
    CrawlComments { url: String, max: usize },
    /// 抓取弹幕
    CrawlDanmaku { url: String, max: usize },
    /// 视频转写
    Transcribe { url: String },
    /// 调用 LLM 总结
    Summarize { prompt: String },
    /// 停止 VM
    StopVm { name: String },
}

/// 完整任务规划。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub steps: Vec<PlanStep>,
}