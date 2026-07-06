//! yt-dlp 字幕下载器。
//!
//! 支持：
//! - B 站（bilibili.com）—— 需要 cookies
//! - YouTube —— 需要 cookies 或公开视频
//! - 其他 yt-dlp 支持的平台
//!
//! 字幕来源优先级：
//! 1. 内嵌字幕（--write-subs）
//! 2. 自动生成字幕（--write-auto-sub）
//! 3. 无字幕时返回视频元信息（标题+描述）

use std::path::{Path, PathBuf};
use std::process::Stdio;
use tempfile::TempDir;

use crate::error::{Error, Result};

/// 从视频 URL 提取出的文本信息。
#[derive(Debug, Clone)]
pub struct VideoText {
    /// 视频标题
    pub title: String,
    /// 视频描述
    pub description: String,
    /// 字幕文本（已清理时间戳，纯文字）
    pub subtitle: Option<String>,
    /// 视频时长（秒）
    pub duration: Option<f64>,
    /// 上传者
    pub uploader: Option<String>,
    /// 平台
    pub platform: String,
}

impl VideoText {
    /// 返回用于交给 Claude 分析的全文。
    pub fn to_analysis_text(&self) -> String {
        let mut parts = Vec::new();

        parts.push(format!("【视频标题】{}", self.title));

        if let Some(up) = &self.uploader {
            parts.push(format!("【UP主/作者】{}", up));
        }
        if let Some(dur) = self.duration {
            let m = (dur as u64) / 60;
            let s = (dur as u64) % 60;
            parts.push(format!("【时长】{}:{:02}", m, s));
        }
        parts.push(format!("【平台】{}", self.platform));

        if !self.description.is_empty() {
            parts.push(format!("【视频简介】\n{}", self.description));
        }

        if let Some(sub) = &self.subtitle {
            if !sub.trim().is_empty() {
                parts.push(format!("【字幕/文字稿】\n{}", sub));
            }
        }

        parts.join("\n\n")
    }
}

/// yt-dlp 字幕下载器。
pub struct SubtitleFetcher {
    /// yt-dlp 可执行路径
    ytdlp_path: PathBuf,
    /// cookies 文件路径（可选）
    cookies_file: Option<PathBuf>,
    /// 超时（秒）
    timeout_secs: u64,
}

impl SubtitleFetcher {
    pub fn new() -> Result<Self> {
        let ytdlp_path = find_ytdlp()?;
        let cookies_file = find_cookies_file();
        Ok(Self {
            ytdlp_path,
            cookies_file,
            timeout_secs: 60,
        })
    }

    pub fn with_cookies(mut self, path: PathBuf) -> Self {
        self.cookies_file = Some(path);
        self
    }

    /// 从 URL 获取视频文字信息（字幕 + 元数据）。
    pub async fn fetch(&self, url: &str) -> Result<VideoText> {
        let tmp = TempDir::new()
            .map_err(|e| Error::Crawler(format!("tempdir failed: {e}")))?;

        // 1. 先获取元信息（标题、描述、时长）
        let meta = self.fetch_metadata(url).await?;

        // 2. 尝试下载字幕
        let subtitle = self.fetch_subtitle(url, tmp.path()).await.ok().flatten();

        let platform = detect_platform(url);

        Ok(VideoText {
            title: meta.title,
            description: meta.description,
            subtitle,
            duration: meta.duration,
            uploader: meta.uploader,
            platform,
        })
    }

    /// 获取视频元信息（不下载视频）。
    async fn fetch_metadata(&self, url: &str) -> Result<MetaInfo> {
        let mut cmd = self.base_cmd();
        cmd.args([
            "--dump-json",
            "--no-playlist",
            "--no-download",
            "--quiet",
            "--no-warnings",
            url,
        ]);

        let output = run_with_timeout(cmd, self.timeout_secs).await?;

        if output.is_empty() {
            return Err(Error::Crawler("yt-dlp returned empty metadata".into()));
        }

        let json: serde_json::Value = serde_json::from_str(&output)
            .map_err(|e| Error::Crawler(format!("invalid metadata JSON: {e}")))?;

        Ok(MetaInfo {
            title: json["title"].as_str().unwrap_or("（未知标题）").to_string(),
            description: json["description"].as_str().unwrap_or("").to_string(),
            duration: json["duration"].as_f64(),
            uploader: json["uploader"].as_str().map(|s| s.to_string()),
        })
    }

    /// 下载字幕并解析为纯文本。
    async fn fetch_subtitle(
        &self,
        url: &str,
        out_dir: &Path,
    ) -> Result<Option<String>> {
        let out_tmpl = out_dir.join("sub.%(ext)s").to_string_lossy().to_string();

        let mut cmd = self.base_cmd();
        cmd.args([
            "--write-subs",       // 内嵌字幕
            "--write-auto-sub",   // 自动字幕（AI 生成）
            "--sub-lang", "zh-Hans,zh,zh-CN,en",
            "--sub-format", "vtt/best",
            "--skip-download",    // 不下载视频
            "--no-playlist",
            "--quiet",
            "--no-warnings",
            "-o", &out_tmpl,
            url,
        ]);

        let _ = run_with_timeout(cmd, self.timeout_secs).await;

        // 查找下载的字幕文件
        let sub_text = find_and_parse_subtitle(out_dir)?;
        Ok(sub_text)
    }

    /// 构建基础命令（注入 cookies）。
    fn base_cmd(&self) -> tokio::process::Command {
        let mut cmd = tokio::process::Command::new(&self.ytdlp_path);
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .arg("--no-update");

        if let Some(cookies) = &self.cookies_file {
            cmd.arg("--cookies").arg(cookies);
        }
        cmd
    }
}

// =============================================
// 辅助结构
// =============================================

struct MetaInfo {
    title: String,
    description: String,
    duration: Option<f64>,
    uploader: Option<String>,
}

/// 查找 yt-dlp 可执行文件。
fn find_ytdlp() -> Result<PathBuf> {
    // 先从 PATH 找
    let names = if cfg!(windows) {
        vec!["yt-dlp.exe", "yt-dlp"]
    } else {
        vec!["yt-dlp"]
    };

    for name in &names {
        if let Some(p) = which(name) {
            return Ok(p);
        }
    }

    // 常见安装路径
    let candidates = vec![
        "D:/software/python/3115/Scripts/yt-dlp.exe",
        "/usr/local/bin/yt-dlp",
        "/usr/bin/yt-dlp",
    ];
    for c in candidates {
        if Path::new(c).exists() {
            return Ok(PathBuf::from(c));
        }
    }

    Err(Error::Crawler(
        "yt-dlp not found. Install with: pip install yt-dlp".into(),
    ))
}

fn which(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            let p = dir.join(name);
            if p.exists() { Some(p) } else { None }
        })
    })
}

/// 查找 cookies 文件（支持多个约定位置）。
fn find_cookies_file() -> Option<PathBuf> {
    let candidates = vec![
        "./cookies.txt",
        "./bilibili_cookies.txt",
        "~/.config/yt-dlp/cookies.txt",
    ];
    candidates
        .into_iter()
        .map(|s| {
            if s.starts_with("~/") {
                if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
                    PathBuf::from(home).join(&s[2..])
                } else {
                    PathBuf::from(s)
                }
            } else {
                PathBuf::from(s)
            }
        })
        .find(|p| p.exists())
}

/// 带超时的异步进程执行。
async fn run_with_timeout(
    mut cmd: tokio::process::Command,
    timeout_secs: u64,
) -> Result<String> {
    let child = cmd.spawn()
        .map_err(|e| Error::Crawler(format!("spawn yt-dlp failed: {e}")))?;

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        child.wait_with_output(),
    )
    .await
    .map_err(|_| Error::Crawler("yt-dlp timed out".into()))?
    .map_err(|e| Error::Crawler(format!("yt-dlp failed: {e}")))?;

    // 优先返回 stdout，如果为空且有 stderr 则返回 stderr（便于 metadata fallback）
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if !stdout.trim().is_empty() {
        return Ok(stdout);
    }

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(Error::Crawler(format!(
            "yt-dlp exit {}: {}",
            output.status.code().unwrap_or(-1),
            stderr.lines().last().unwrap_or("unknown error")
        )));
    }

    Ok(stdout)
}

/// 在目录中查找字幕文件并解析。
fn find_and_parse_subtitle(dir: &Path) -> Result<Option<String>> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| Error::Crawler(format!("read dir failed: {e}")))?;

    for entry in entries.flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match ext {
            "vtt" => {
                let raw = std::fs::read_to_string(&path)
                    .map_err(|e| Error::Crawler(format!("read vtt failed: {e}")))?;
                return Ok(Some(parse_vtt(&raw)));
            }
            "srt" => {
                let raw = std::fs::read_to_string(&path)
                    .map_err(|e| Error::Crawler(format!("read srt failed: {e}")))?;
                return Ok(Some(parse_srt(&raw)));
            }
            _ => {}
        }
    }
    Ok(None)
}

/// 解析 WebVTT 字幕为纯文本（去掉时间戳和 HTML tag）。
pub fn parse_vtt(raw: &str) -> String {
    let mut lines = Vec::new();
    let mut in_cue = false;

    for line in raw.lines() {
        let line = line.trim();

        // 跳过文件头
        if line == "WEBVTT" || line.starts_with("NOTE") || line.starts_with("STYLE") {
            in_cue = false;
            continue;
        }

        // 时间戳行：00:00:00.000 --> 00:00:05.000
        if line.contains("-->") {
            in_cue = true;
            continue;
        }

        // 空行表示 cue 结束
        if line.is_empty() {
            in_cue = false;
            continue;
        }

        if in_cue {
            // 去掉 <tag> HTML 标签
            let text = strip_html_tags(line);
            if !text.is_empty() {
                lines.push(text);
            }
        }
    }

    // 去重连续重复行（自动字幕常见）
    dedup_lines(lines)
}

/// 解析 SRT 字幕为纯文本。
pub fn parse_srt(raw: &str) -> String {
    let mut lines = Vec::new();
    let mut skip_next = false;

    for line in raw.lines() {
        let line = line.trim();

        if line.is_empty() {
            skip_next = false;
            continue;
        }

        // 序号行（纯数字）
        if line.chars().all(|c| c.is_ascii_digit()) {
            skip_next = true; // 下一行是时间戳
            continue;
        }

        // 时间戳行
        if skip_next && line.contains("-->") {
            skip_next = false;
            continue;
        }

        let text = strip_html_tags(line);
        if !text.is_empty() {
            lines.push(text);
        }
    }

    dedup_lines(lines)
}

fn strip_html_tags(s: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(c),
            _ => {}
        }
    }
    result.trim().to_string()
}

fn dedup_lines(lines: Vec<String>) -> String {
    let mut out: Vec<String> = Vec::new();
    for line in lines {
        if out.last().map(|l| l != &line).unwrap_or(true) {
            out.push(line);
        }
    }
    out.join(" ")
}

fn detect_platform(url: &str) -> String {
    if url.contains("bilibili.com") || url.contains("b23.tv") {
        "哔哩哔哩 (Bilibili)".to_string()
    } else if url.contains("youtube.com") || url.contains("youtu.be") {
        "YouTube".to_string()
    } else if url.contains("weibo.com") {
        "微博".to_string()
    } else if url.contains("douyin.com") || url.contains("tiktok.com") {
        "抖音/TikTok".to_string()
    } else {
        "未知平台".to_string()
    }
}
