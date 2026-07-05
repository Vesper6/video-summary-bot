//! 视频内容总结模块。
//!
//! - [`asr`] — 语音转文字（Whisper）
//! - [`timeline`] — 时间线分段

pub mod asr;
pub mod timeline;

use serde::{Deserialize, Serialize};

/// 时间线分段。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineSegment {
    /// 起始时间（秒）
    pub start: f32,
    /// 结束时间（秒）
    pub end: f32,
    /// 段标题
    pub title: String,
    /// 段摘要
    pub summary: String,
}

/// 视频总结结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    /// 视频标题
    pub title: String,
    /// 视频 URL
    pub url: String,
    /// 时间线分段
    pub timeline: Vec<TimelineSegment>,
    /// 整体摘要
    pub overall_summary: String,
    /// 关键要点
    pub key_points: Vec<String>,
}