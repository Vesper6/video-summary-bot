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
    /// 简要（3-5 个要点）
    Light,
    /// 标准（时间线 + 要点 + 评价）
    Standard,
    /// 深度（完整分析）
    Full,
}

impl CrawlLevel {
    /// 档位的小写字符串表示。
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
    /// 评论 + 弹幕
    Both,
}

/// summarize 命令参数。
#[derive(Debug, clap::Args)]
pub struct SummarizeCmd {
    /// 视频 URL（与 --text/--text-file 二选一）
    #[arg(long)]
    pub url: Option<String>,

    /// 直接传入文字稿/字幕文本，跳过 yt-dlp
    #[arg(long)]
    pub text: Option<String>,

    /// 从文件读取文字稿（.txt / .srt / .vtt）
    #[arg(long)]
    pub text_file: Option<std::path::PathBuf>,

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
    /// 视频 URL
    #[arg(long)]
    pub url: String,
    /// 抓取类型（评论 / 弹幕 / 两者）
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

/// 运行 summarize 子命令。
pub async fn run_summarize(cmd: SummarizeCmd, _config: &AppConfig) -> Result<i32> {
    let claude = ClaudeClient::from_env()?;

    // ── 参数校验 ────────────────────────────────
    if cmd.url.is_none() && cmd.text.is_none() && cmd.text_file.is_none() {
        eprintln!("❌ 必须提供 --url、--text 或 --text-file 之一");
        eprintln!("   例：vsb summarize --url https://www.bilibili.com/video/BVxxx");
        eprintln!("   例：vsb summarize --text-file subtitle.vtt");
        return Ok(1);
    }

    println!("🤖 模型：{}", claude.model());

    // ── 获取分析内容 ────────────────────────────

    // 模式 A：直接传入文字稿
    if let Some(text) = &cmd.text {
        return summarize_text(&claude, text, None, &cmd).await;
    }

    // 模式 B：从文件读取
    if let Some(path) = &cmd.text_file {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| Error::Other(format!("read file failed: {e}")))?;
        let text = if path.extension().and_then(|e| e.to_str()) == Some("vtt") {
            crate::crawler::subtitle::parse_vtt(&raw)
        } else if path.extension().and_then(|e| e.to_str()) == Some("srt") {
            crate::crawler::subtitle::parse_srt(&raw)
        } else {
            raw
        };
        println!("📂 文件：{}", path.display());
        return summarize_text(&claude, &text, None, &cmd).await;
    }

    // 模式 C：URL 模式
    let url = cmd.url.as_deref().unwrap();
    println!("🔍 视频：{url}");

    let video_text = if cmd.no_subtitle {
        None
    } else {
        print!("📥 正在用 yt-dlp 获取字幕/元数据...");
        match SubtitleFetcher::new() {
            Err(e) => {
                println!(" ⚠️  yt-dlp 不可用（{e}），直接让 Claude 分析");
                None
            }
            Ok(mut fetcher) => {
                if let Some(cookies) = &cmd.cookies {
                    fetcher = fetcher.with_cookies(cookies.clone());
                }
                match fetcher.fetch(url).await {
                    Ok(vt) => {
                        println!(" ✅");
                        println!("📌 标题：{}", vt.title);
                        if let Some(dur) = vt.duration {
                            println!("⏱  时长：{}:{:02}", dur as u64 / 60, dur as u64 % 60);
                        }
                        if vt.subtitle.is_some() {
                            println!("📝 字幕：已获取");
                        } else {
                            println!("📝 字幕：未找到（将使用标题+简介）");
                        }
                        Some(vt)
                    }
                    Err(e) => {
                        println!(" ⚠️  获取失败（{e}），直接让 Claude 分析");
                        None
                    }
                }
            }
        }
    };

    // 构造 prompt
    let (prompt_text, title) = match &video_text {
        Some(vt) => (vt.to_analysis_text(), Some(vt.title.clone())),
        None => (
            format!(
                "视频链接：{url}\n\n注意：无法直接获取视频内容，请根据已有知识尽力分析，\
                 并在报告开头说明限制。"
            ),
            None,
        ),
    };

    println!("─────────────────────────────────────");
    let result = call_claude(&claude, &prompt_text, &cmd).await?;
    print_and_save(&result, url, title.as_deref(), &cmd.output)?;
    Ok(0)
}

/// 分析纯文字稿。
async fn summarize_text(
    claude: &ClaudeClient,
    text: &str,
    title: Option<&str>,
    cmd: &SummarizeCmd,
) -> Result<i32> {
    let char_count = text.chars().count();
    println!(
        "📝 文字稿：{} 字{}",
        char_count,
        if char_count > 10000 {
            "（较长，将截取前 10000 字）"
        } else {
            ""
        }
    );
    let text = if char_count > 10000 {
        &text.chars().take(10000).collect::<String>()
    } else {
        text
    };

    let prompt = format!("以下是视频文字稿，请生成结构化总结：\n\n{text}");

    println!("─────────────────────────────────────");
    let result = call_claude(claude, &prompt, cmd).await?;
    let url = cmd.url.as_deref().unwrap_or("（文字稿）");
    print_and_save(&result, url, title, &cmd.output)?;
    Ok(0)
}

/// 构造总结 system prompt（CLI 与 HTTP API 共用）。
fn build_system_prompt(language: &str) -> String {
    format!("你是专业的视频内容分析助手，擅长生成结构化的视频总结报告。请用{language}输出。")
}

/// 构造总结 user prompt（CLI 与 HTTP API 共用）。
fn build_user_prompt(level: CrawlLevel, content: &str) -> String {
    let depth = match level {
        CrawlLevel::Light => "简要总结（3-5 个核心要点）",
        CrawlLevel::Standard => "标准总结（时间线分段 + 关键要点 + 总体评价）",
        CrawlLevel::Full => "深度总结（完整时间线 + 详细分析 + 总体评价）",
    };
    format!(
        "请生成{depth}：\n\n{content}\n\n---\n请按照以下结构输出：\n\
         ## 📌 视频基本信息\n\
         ## 🎯 核心内容总结\n\
         ## 📋 时间线 / 内容分段\n\
         ## 💡 关键要点\n\
         ## ⭐ 总体评价"
    )
}

/// 调用 Claude 生成总结。
async fn call_claude(claude: &ClaudeClient, content: &str, cmd: &SummarizeCmd) -> Result<String> {
    let system = build_system_prompt(&cmd.language);
    let user_prompt = build_user_prompt(cmd.level, content);

    println!("💬 正在分析...\n");
    claude.send_with_system(&system, &user_prompt).await
}

// =============================================
// HTTP API 入口（POST /api/summarize，GUI「视频总结」页使用）
// =============================================

/// `POST /api/summarize` 请求体。
#[derive(Debug, serde::Deserialize)]
pub struct SummarizeApiRequest {
    /// 视频 URL
    pub url: String,
    /// 抓取档位（默认 standard）
    pub level: Option<CrawlLevel>,
    /// 输出语言（默认 zh-CN）
    pub language: Option<String>,
    /// 跳过 yt-dlp 字幕下载
    pub no_subtitle: Option<bool>,
}

/// `POST /api/summarize` 响应体。
#[derive(Debug, serde::Serialize)]
pub struct SummarizeApiResponse {
    /// 视频标题（yt-dlp 获取到时返回）
    pub title: Option<String>,
    /// 使用的模型
    pub model: String,
    /// Markdown 总结报告
    pub markdown: String,
    /// 是否成功获取到字幕
    pub subtitle_fetched: bool,
}

/// GUI / HTTP API 的视频总结入口（不打印到 stdout）。
pub async fn summarize_api(req: SummarizeApiRequest) -> Result<SummarizeApiResponse> {
    let claude = ClaudeClient::from_env()?;
    let level = req.level.unwrap_or(CrawlLevel::Standard);
    let language = req.language.unwrap_or_else(|| "zh-CN".to_string());
    let no_subtitle = req.no_subtitle.unwrap_or(false);
    let url = req.url.trim();
    if url.is_empty() {
        return Err(Error::Other("url is required".into()));
    }

    let video_text = if no_subtitle {
        None
    } else {
        match SubtitleFetcher::new() {
            Ok(fetcher) => match fetcher.fetch(url).await {
                Ok(vt) => Some(vt),
                Err(e) => {
                    tracing::warn!("subtitle fetch failed, falling back to URL-only: {e}");
                    None
                }
            },
            Err(e) => {
                tracing::warn!("yt-dlp unavailable, falling back to URL-only: {e}");
                None
            }
        }
    };

    let (content, title) = match &video_text {
        Some(vt) => (vt.to_analysis_text(), Some(vt.title.clone())),
        None => (
            format!(
                "视频链接：{url}\n\n注意：无法直接获取视频内容，请根据已有知识尽力分析，\
                 并在报告开头说明限制。"
            ),
            None,
        ),
    };

    let system = build_system_prompt(&language);
    let user_prompt = build_user_prompt(level, &content);
    let markdown = claude.send_with_system(&system, &user_prompt).await?;

    Ok(SummarizeApiResponse {
        title,
        model: claude.model().to_string(),
        markdown,
        subtitle_fetched: video_text
            .as_ref()
            .map(|v| v.subtitle.is_some())
            .unwrap_or(false),
    })
}

/// 打印结果 + 可选保存文件。
fn print_and_save(
    result: &str,
    url: &str,
    title: Option<&str>,
    output: &Option<std::path::PathBuf>,
) -> Result<()> {
    println!("{result}");
    println!("\n─────────────────────────────────────");

    if let Some(output_dir) = output {
        let slug = title.map(slugify).unwrap_or_else(|| slugify(url));
        std::fs::create_dir_all(output_dir)
            .map_err(|e| Error::Other(format!("create dir failed: {e}")))?;
        let path = output_dir.join(format!("{slug}.md"));
        let md = format!("# 视频总结\n\n**URL**: {url}\n\n---\n\n{result}");
        std::fs::write(&path, &md).map_err(|e| Error::Other(format!("write failed: {e}")))?;
        println!("✅ 已保存：{}", path.display());
    }
    Ok(())
}

/// 运行 crawl 子命令（尚未实现，依赖 micro VM）。
pub async fn run_crawl(cmd: CrawlCmd, _config: &AppConfig) -> Result<i32> {
    eprintln!("❌ crawl 命令需要 micro VM 支持，尚未实现。");
    eprintln!(
        "   提示：使用 `vsb summarize --url {}` 获取视频总结。",
        cmd.url
    );
    Ok(1)
}

fn slugify(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .chars()
        .take(60)
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}
