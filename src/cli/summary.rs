//! 视频总结子命令。

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

    /// 开启视觉分析：下载视频并抽帧，让 Claude 结合画面总结（需 yt-dlp + ffmpeg）
    #[arg(long)]
    pub vision: bool,

    /// 视觉抽帧间隔（秒），仅 --vision 时生效
    #[arg(long, default_value_t = crate::crawler::vision::DEFAULT_INTERVAL_SECS)]
    pub vision_interval: u32,
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

    // yt-dlp 下载器（字幕或视觉都需要）
    let fetcher = if cmd.no_subtitle && !cmd.vision {
        None
    } else {
        match SubtitleFetcher::new() {
            Ok(mut f) => {
                if let Some(cookies) = &cmd.cookies {
                    f = f.with_cookies(cookies.clone());
                }
                Some(f)
            }
            Err(e) => {
                println!("⚠️  yt-dlp 不可用（{e}）");
                None
            }
        }
    };

    // 字幕 / 元数据
    let video_text = if cmd.no_subtitle {
        None
    } else if let Some(f) = &fetcher {
        print!("📥 正在用 yt-dlp 获取字幕/元数据...");
        match f.fetch(url).await {
            Ok(vt) => {
                println!(" ✅");
                println!("📌 标题：{}", vt.title);
                if let Some(dur) = vt.duration {
                    println!("⏱  时长：{}:{:02}", dur as u64 / 60, dur as u64 % 60);
                }
                println!(
                    "📝 字幕：{}",
                    if vt.subtitle.is_some() {
                        "已获取"
                    } else {
                        "未找到（将使用标题+简介）"
                    }
                );
                Some(vt)
            }
            Err(e) => {
                println!(" ⚠️  获取失败（{e}）");
                None
            }
        }
    } else {
        None
    };

    // 视觉抽帧（--vision）
    let frames = if cmd.vision {
        capture_frames(fetcher.as_ref(), url, cmd.vision_interval, &video_text).await
    } else {
        None
    };

    // 文字内容
    let (text_content, title) = match &video_text {
        Some(vt) => (vt.to_analysis_text(), Some(vt.title.clone())),
        None => (
            format!(
                "视频链接：{url}\n\n注意：无法直接获取视频文字内容，请结合{}尽力分析，\
                 并在报告开头说明限制。",
                if frames.is_some() {
                    "画面截图"
                } else {
                    "已有知识"
                }
            ),
            None,
        ),
    };

    println!("─────────────────────────────────────");
    let system = build_system_prompt(&cmd.language);
    let result = if let Some(fs) = &frames {
        let note = format!(
            "\n\n【视频画面】随本消息附带 {} 张视频截图（每 {} 秒 1 帧，按时间先后排列）。\
             请结合字幕文字与画面内容综合总结，画面能补充字幕没有的信息（场景、图表、演示等）。",
            fs.images.len(),
            fs.interval_secs
        );
        let prompt = build_user_prompt(cmd.level, &format!("{text_content}{note}"));
        println!("💬 正在结合画面分析...\n");
        claude
            .send_with_images(&system, &prompt, &fs.images)
            .await?
    } else {
        call_claude(&claude, &text_content, &cmd).await?
    };
    print_and_save(&result, url, title.as_deref(), &cmd.output)?;
    Ok(0)
}

/// 下载视频并抽帧（失败返回 None，降级为纯字幕分析）。
async fn capture_frames(
    fetcher: Option<&SubtitleFetcher>,
    url: &str,
    interval: u32,
    video_text: &Option<crate::crawler::subtitle::VideoText>,
) -> Option<crate::crawler::vision::FrameSet> {
    let Some(f) = fetcher else {
        println!("⚠️  --vision 需要 yt-dlp，跳过画面分析");
        return None;
    };
    let tmp = match tempfile::TempDir::new() {
        Ok(t) => t,
        Err(e) => {
            println!("⚠️  创建临时目录失败（{e}），跳过画面分析");
            return None;
        }
    };
    println!("🎞️  正在下载视频并抽帧（每 {interval} 秒 1 帧）...");
    let video = match f.download_video(url, tmp.path()).await {
        Ok(v) => v,
        Err(e) => {
            println!("⚠️  视频下载失败（{e}），跳过画面分析");
            return None;
        }
    };
    let duration = video_text.as_ref().and_then(|v| v.duration);
    match crate::crawler::vision::extract_frames(&video, f.ffmpeg_path(), interval, duration).await
    {
        Ok(fs) => {
            println!(
                "✅ 抽取 {} 帧（间隔 {} 秒）",
                fs.images.len(),
                fs.interval_secs
            );
            Some(fs)
        }
        Err(e) => {
            println!("⚠️  抽帧失败（{e}），跳过画面分析");
            None
        }
    }
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
    /// 开启视觉分析（下载视频抽帧）
    pub vision: Option<bool>,
    /// 视觉抽帧间隔（秒）
    pub vision_interval: Option<u32>,
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
    /// 视觉分析实际使用的画面帧数（0 = 未做视觉分析）
    pub frames_analyzed: usize,
}

/// GUI / HTTP API 的视频总结入口（不打印到 stdout）。
pub async fn summarize_api(req: SummarizeApiRequest) -> Result<SummarizeApiResponse> {
    let claude = ClaudeClient::from_env()?;
    let level = req.level.unwrap_or(CrawlLevel::Standard);
    let language = req.language.unwrap_or_else(|| "zh-CN".to_string());
    let no_subtitle = req.no_subtitle.unwrap_or(false);
    let vision = req.vision.unwrap_or(false);
    let vision_interval = req
        .vision_interval
        .unwrap_or(crate::crawler::vision::DEFAULT_INTERVAL_SECS);
    let url = req.url.trim();
    if url.is_empty() {
        return Err(Error::Other("url is required".into()));
    }

    let fetcher = if no_subtitle && !vision {
        None
    } else {
        SubtitleFetcher::new()
            .map_err(|e| tracing::warn!("yt-dlp unavailable: {e}"))
            .ok()
    };

    let video_text = if no_subtitle {
        None
    } else if let Some(f) = &fetcher {
        f.fetch(url)
            .await
            .map_err(|e| tracing::warn!("subtitle fetch failed: {e}"))
            .ok()
    } else {
        None
    };

    let frames = if vision {
        match &fetcher {
            Some(f) => {
                capture_frames_quiet(
                    f,
                    url,
                    vision_interval,
                    video_text.as_ref().and_then(|v| v.duration),
                )
                .await
            }
            None => None,
        }
    } else {
        None
    };

    let (content, title) = match &video_text {
        Some(vt) => (vt.to_analysis_text(), Some(vt.title.clone())),
        None => (
            format!(
                "视频链接：{url}\n\n注意：无法直接获取视频文字内容，请结合{}尽力分析，\
                 并在报告开头说明限制。",
                if frames.is_some() {
                    "画面截图"
                } else {
                    "已有知识"
                }
            ),
            None,
        ),
    };

    let system = build_system_prompt(&language);
    let markdown = if let Some(fs) = &frames {
        let note = format!(
            "\n\n【视频画面】随本消息附带 {} 张视频截图（每 {} 秒 1 帧，按时间先后排列）。\
             请结合字幕文字与画面内容综合总结。",
            fs.images.len(),
            fs.interval_secs
        );
        let prompt = build_user_prompt(level, &format!("{content}{note}"));
        claude
            .send_with_images(&system, &prompt, &fs.images)
            .await?
    } else {
        let prompt = build_user_prompt(level, &content);
        claude.send_with_system(&system, &prompt).await?
    };

    Ok(SummarizeApiResponse {
        title,
        model: claude.model().to_string(),
        markdown,
        subtitle_fetched: video_text
            .as_ref()
            .map(|v| v.subtitle.is_some())
            .unwrap_or(false),
        frames_analyzed: frames.as_ref().map(|f| f.images.len()).unwrap_or(0),
    })
}

/// 下载视频并抽帧（HTTP API 用，静默；失败降级为 None）。
async fn capture_frames_quiet(
    fetcher: &SubtitleFetcher,
    url: &str,
    interval: u32,
    duration: Option<f64>,
) -> Option<crate::crawler::vision::FrameSet> {
    let tmp = tempfile::TempDir::new().ok()?;
    let video = fetcher
        .download_video(url, tmp.path())
        .await
        .map_err(|e| tracing::warn!("video download failed: {e}"))
        .ok()?;
    crate::crawler::vision::extract_frames(&video, fetcher.ffmpeg_path(), interval, duration)
        .await
        .map_err(|e| tracing::warn!("frame extraction failed: {e}"))
        .ok()
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
