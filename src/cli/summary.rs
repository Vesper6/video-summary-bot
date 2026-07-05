//! 视频总结 / 抓取子命令。

use clap::ValueEnum;

use crate::config::AppConfig;
use crate::error::Result;

/// 抓取档位。
#[derive(Debug, Clone, Copy, ValueEnum, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CrawlLevel {
    /// 100 条
    Light,
    /// 1,000 条
    Standard,
    /// 全部
    Full,
}

impl CrawlLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            CrawlLevel::Light => "light",
            CrawlLevel::Standard => "standard",
            CrawlLevel::Full => "full",
        }
    }
}

/// 抓取类型。
#[derive(Debug, Clone, Copy, ValueEnum, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CrawlType {
    /// 仅评论
    Comments,
    /// 仅弹幕
    Danmaku,
    /// 两者都抓
    Both,
}

/// summarize 命令参数。
#[derive(Debug, clap::Args)]
pub struct SummarizeCmd {
    /// 视频 URL
    #[arg(long)]
    pub url: String,

    /// 抓取档位
    #[arg(long, value_enum, default_value_t = CrawlLevel::Standard)]
    pub level: CrawlLevel,

    /// 输出语言
    #[arg(long, default_value = "zh-CN")]
    pub language: String,

    /// 输出目录
    #[arg(long)]
    pub output: Option<std::path::PathBuf>,
}

/// crawl 命令参数。
#[derive(Debug, clap::Args)]
pub struct CrawlCmd {
    /// 视频 URL
    #[arg(long)]
    pub url: String,

    /// 抓取类型
    #[arg(long, value_enum, default_value_t = CrawlType::Both)]
    #[arg(rename_all = "kebab-case")]
    pub r#type: CrawlType,

    /// 抓取档位
    #[arg(long, value_enum, default_value_t = CrawlLevel::Standard)]
    pub level: CrawlLevel,

    /// 输出目录
    #[arg(long)]
    pub output: Option<std::path::PathBuf>,
}

/// summarize 命令执行入口。
pub async fn run_summarize(cmd: SummarizeCmd, _config: &AppConfig) -> Result<i32> {
    tracing::info!(
        "summarize: url={}, level={}, lang={}",
        cmd.url,
        cmd.level.as_str(),
        cmd.language
    );

    // 1. 启动 VM
    // 2. 在 VM 内运行 Claude Code 抓取 + 总结
    // 3. 收集结果

    tracing::warn!("summarize: not yet implemented");
    Ok(0)
}

/// crawl 命令执行入口。
pub async fn run_crawl(cmd: CrawlCmd, _config: &AppConfig) -> Result<i32> {
    tracing::info!(
        "crawl: url={}, type={:?}, level={}",
        cmd.url,
        cmd.r#type,
        cmd.level.as_str()
    );

    tracing::warn!("crawl: not yet implemented");
    Ok(0)
}