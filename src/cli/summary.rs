//! 视频总结 / 抓取子命令。

use clap::ValueEnum;

use crate::agents::claude::ClaudeClient;
use crate::config::AppConfig;
use crate::error::{Error, Result};

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

// =============================================
// summarize 核心实现
// =============================================

/// summarize 命令执行入口。
pub async fn run_summarize(cmd: SummarizeCmd, _config: &AppConfig) -> Result<i32> {
    tracing::info!(
        "summarize: url={} level={} lang={}",
        cmd.url,
        cmd.level.as_str(),
        cmd.language
    );

    // 初始化 Claude 客户端
    let claude = ClaudeClient::from_env()?;
    tracing::info!("using model: {}", claude.model());

    // 构造 system prompt
    let system = format!(
        "你是一个专业的视频内容分析助手。\
         用户会给你一个视频链接，请对该视频进行全面分析并生成结构化的总结报告。\
         请用{}输出。",
        cmd.language
    );

    // 构造 user prompt
    let prompt = build_summarize_prompt(&cmd.url, &cmd.level);

    println!("🔍 正在分析视频：{}", cmd.url);
    println!("📊 档位：{}", cmd.level.as_str());
    println!("🤖 模型：{}", claude.model());
    println!("─────────────────────────────────────");

    // 调用 Claude API
    let result = claude.send_with_system(&system, &prompt).await?;

    // 输出结果
    println!("{}", result);
    println!("─────────────────────────────────────");

    // 保存到文件（如果指定了输出目录）
    if let Some(output_dir) = &cmd.output {
        let filename = url_to_filename(&cmd.url);
        std::fs::create_dir_all(output_dir)
            .map_err(|e| Error::Other(format!("create output dir failed: {e}")))?;
        let path = output_dir.join(format!("{filename}.md"));
        std::fs::write(&path, &result)
            .map_err(|e| Error::Other(format!("write output failed: {e}")))?;
        println!("✅ 已保存到：{}", path.display());
    }

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
    tracing::warn!("crawl: VM-based crawler not yet implemented (requires running micro VM)");
    eprintln!("❌ crawl 命令需要 micro VM 支持，尚未实现。");
    Ok(1)
}

// =============================================
// 辅助函数
// =============================================

fn build_summarize_prompt(url: &str, level: &CrawlLevel) -> String {
    let depth = match level {
        CrawlLevel::Light => "简要总结（3-5 个要点）",
        CrawlLevel::Standard => "标准总结（时间线分段 + 关键要点 + 总体评价）",
        CrawlLevel::Full => "深度总结（完整时间线 + 详细分析 + 评论区情绪分析 + 总体评价）",
    };

    format!(
        r#"请分析以下视频并生成{depth}：

视频链接：{url}

请按照以下结构输出：

## 📌 视频基本信息
- 标题：（如能获取）
- 平台：（根据链接判断）
- 链接：{url}

## 🎯 核心内容总结
（用简洁的语言概括视频的主要内容）

## 📋 时间线分段
（按照视频的内容段落，列出各段的时间点和主题）

## 💡 关键要点
（列出视频中最重要的观点、信息或结论）

## 🏷️ 标签与分类
（给视频打上适合的标签）

## ⭐ 总体评价
（内容质量、信息密度、适合人群等）

---
注意：如果无法直接访问该视频，请根据 URL 信息和已有知识尽量提供有价值的分析，并说明限制。"#
    )
}

fn url_to_filename(url: &str) -> String {
    url.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
        .chars()
        .take(80)
        .collect()
}
