//! 视频总结 / 抓取子命令。

use clap::ValueEnum;

use crate::agents::claude::ClaudeClient;
use crate::config::AppConfig;
use crate::crawler::subtitle::SubtitleFetcher;
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
    Comments,
    Danmaku,
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

    /// 输出目录（指定后保存为 Markdown 文件）
    #[arg(long)]
    pub output: Option<std::path::PathBuf>,

    /// cookies 文件路径（用于访问需要登录的平台）
    #[arg(long)]
    pub cookies: Option<std::path::PathBuf>,

    /// 跳过字幕下载，直接让 Claude 分析 URL（无需 yt-dlp）
    #[arg(long)]
    pub no_subtitle: bool,
}

/// crawl 命令参数。
#[derive(Debug, clap::Args)]
pub struct CrawlCmd {
    #[arg(long)]
    pub url: String,
    #[arg(long, value_enum, default_value_t = CrawlType::Both)]
    #[arg(rename_all = "kebab-case")]
    pub r#type: CrawlType,
    #[arg(long, value_enum, default_value_t = CrawlLevel::Standard)]
    pub level: CrawlLevel,
    #[arg(long)]
    pub output: Option<std::path::PathBuf>,
}

// =============================================
// summarize 核心实现
// =============================================

pub async fn run_summarize(cmd: SummarizeCmd, _config: &AppConfig) -> Result<i32> {
    let claude = ClaudeClient::from_env()?;

    println!("🔍 视频：{}", cmd.url);
    println!("🤖 模型：{}", claude.model());

    // ── 第一步：获取视频文字内容 ─────────────────
    let video_text = if cmd.no_subtitle {
        // 直接跳过字幕，让 Claude 根据 URL 分析
        None
    } else {
        print!("📥 正在用 yt-dlp 获取字幕/元数据...");
        match SubtitleFetcher::new() {
            Err(e) => {
                println!(" ⚠️  yt-dlp 不可用（{}），直接让 Claude 分析", e);
                None
            }
            Ok(mut fetcher) => {
                if let Some(cookies) = &cmd.cookies {
                    fetcher = fetcher.with_cookies(cookies.clone());
                }
                match fetcher.fetch(&cmd.url).await {
                    Ok(vt) => {
                        println!(" ✅");
                        println!("📌 标题：{}", vt.title);
                        if let Some(dur) = vt.duration {
                            let m = dur as u64 / 60;
                            let s = dur as u64 % 60;
                            println!("⏱  时长：{}:{:02}", m, s);
                        }
                        if vt.subtitle.is_some() {
                            println!("📝 字幕：已获取");
                        } else {
                            println!("📝 字幕：未找到（将使用标题+简介）");
                        }
                        Some(vt)
                    }
                    Err(e) => {
                        println!(" ⚠️  获取失败（{}），直接让 Claude 分析", e);
                        None
                    }
                }
            }
        }
    };

    // ── 第二步：构造 prompt ─────────────────────
    println!("─────────────────────────────────────");

    let system = format!(
        "你是专业的视频内容分析助手，擅长生成结构化的视频总结报告。请用{}输出。",
        cmd.language
    );

    let user_prompt = match &video_text {
        Some(vt) => {
            let content = vt.to_analysis_text();
            let depth = match cmd.level {
                CrawlLevel::Light => "简要总结（3-5 个核心要点）",
                CrawlLevel::Standard => "标准总结（时间线分段 + 关键要点 + 总体评价）",
                CrawlLevel::Full => "深度总结（完整时间线 + 详细分析 + 总体评价）",
            };
            format!(
                "请根据以下视频信息生成{}：\n\n{}\n\n---\n请按照以下结构输出：\n\
                ## 📌 视频基本信息\n\
                ## 🎯 核心内容总结\n\
                ## 📋 时间线 / 内容分段\n\
                ## 💡 关键要点\n\
                ## ⭐ 总体评价",
                depth, content
            )
        }
        None => {
            format!(
                "请分析以下视频链接并生成结构化总结（注意：你可能无法直接访问该链接，请尽力根据已有信息分析）：\n\n\
                视频链接：{}\n\n\
                请按照以下结构输出：\n\
                ## 📌 视频基本信息\n\
                ## 🎯 核心内容总结\n\
                ## 💡 关键要点\n\
                ## ⭐ 总体评价",
                cmd.url
            )
        }
    };

    // ── 第三步：调用 Claude ────────────────────
    println!("💬 正在分析...\n");
    let result = claude.send_with_system(&system, &user_prompt).await?;
    println!("{}", result);
    println!("\n─────────────────────────────────────");

    // ── 第四步：保存结果 ─────────────────────
    if let Some(output_dir) = &cmd.output {
        let title_slug = video_text
            .as_ref()
            .map(|v| slugify(&v.title))
            .unwrap_or_else(|| slugify(&cmd.url));
        std::fs::create_dir_all(output_dir)
            .map_err(|e| Error::Other(format!("create dir failed: {e}")))?;
        let path = output_dir.join(format!("{}.md", title_slug));
        let content = format!(
            "# 视频总结\n\n**URL**: {}\n\n---\n\n{}",
            cmd.url, result
        );
        std::fs::write(&path, &content)
            .map_err(|e| Error::Other(format!("write failed: {e}")))?;
        println!("✅ 已保存：{}", path.display());
    }

    Ok(0)
}

pub async fn run_crawl(cmd: CrawlCmd, _config: &AppConfig) -> Result<i32> {
    eprintln!("❌ crawl 命令需要 micro VM 支持，尚未实现。");
    eprintln!("   提示：使用 `vsb summarize --url {}` 获取视频总结。", cmd.url);
    Ok(1)
}

// =============================================
// 辅助函数
// =============================================

fn slugify(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
        .collect::<String>()
        .chars()
        .take(60)
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}
