//! 视频数据抓取模块（在 VM 内运行）。
//!
//! - [`subtitle`] — yt-dlp 字幕/元数据下载
//! - [`comments`] — 评论抓取
//! - [`danmaku`] — 弹幕抓取

pub mod comments;
pub mod danmaku;
pub mod subtitle;

use serde::{Deserialize, Serialize};

/// 抓取任务参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlTask {
    /// 视频 URL
    pub url: String,
    /// 抓取档位
    pub level: String,
}