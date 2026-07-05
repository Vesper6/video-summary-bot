//! 视频评论抓取（在 VM 内的 headless Chromium 中执行）。

use serde::{Deserialize, Serialize};

/// 单条评论。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    /// 评论 ID
    pub id: String,
    /// 用户名
    pub username: String,
    /// 评论内容
    pub content: String,
    /// 点赞数
    pub likes: u32,
    /// 发布时间（Unix timestamp）
    pub timestamp: i64,
}

/// 评论抓取器（占位实现）。
pub struct CommentsCrawler {
    pub video_url: String,
    pub max_count: usize,
}

impl CommentsCrawler {
    pub fn new(video_url: String, max_count: usize) -> Self {
        Self { video_url, max_count }
    }

    /// 抓取评论（在 VM 内调用 headless Chromium）。
    pub async fn crawl(&self) -> crate::error::Result<Vec<Comment>> {
        tracing::warn!("comments crawler: not yet implemented");
        Ok(Vec::new())
    }
}