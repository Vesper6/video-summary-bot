//! 视频弹幕抓取（在 VM 内的 headless Chromium 中执行）。

use serde::{Deserialize, Serialize};

/// 单条弹幕。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Danmaku {
    /// 弹幕内容
    pub content: String,
    /// 出现时间（秒，相对视频开头）
    pub time_offset: f32,
    /// 发送时间（Unix timestamp）
    pub timestamp: i64,
    /// 颜色
    pub color: u32,
    /// 字号
    pub font_size: u8,
}

/// 弹幕抓取器（占位实现）。
pub struct DanmakuCrawler {
    pub video_url: String,
    pub max_count: usize,
}

impl DanmakuCrawler {
    pub fn new(video_url: String, max_count: usize) -> Self {
        Self { video_url, max_count }
    }

    /// 抓取弹幕（在 VM 内调用 headless Chromium）。
    pub async fn crawl(&self) -> crate::error::Result<Vec<Danmaku>> {
        tracing::warn!("danmaku crawler: not yet implemented");
        Ok(Vec::new())
    }
}