//! 视频画面抽帧（ffmpeg）+ base64 编码，供 Claude 视觉分析。
//!
//! 流程：yt-dlp 下载视频 → 本模块按固定间隔抽帧（默认每 30 秒 1 帧）→
//! JPEG base64 → 交给 [`crate::agents::claude::ClaudeClient::send_with_images`]。
//!
//! 帧数有硬上限（[`MAX_FRAMES`]）：超长视频会自动拉大间隔，避免一次塞进
//! 几百张图把 token 打爆。

use std::path::{Path, PathBuf};
use std::process::Stdio;

use base64::Engine;

use crate::agents::claude::ImageInput;
use crate::error::{Error, Result};

/// 默认抽帧间隔（秒）。
pub const DEFAULT_INTERVAL_SECS: u32 = 30;

/// 单次分析的最大帧数（超过则自动拉大间隔）。
pub const MAX_FRAMES: usize = 40;

/// 抽帧结果。
pub struct FrameSet {
    /// base64 编码的帧图片（按时间顺序）
    pub images: Vec<ImageInput>,
    /// 实际使用的抽帧间隔（秒）
    pub interval_secs: u32,
}

/// 从视频抽帧并编码为 base64 JPEG。
///
/// - `video`：本地视频文件路径
/// - `ffmpeg`：ffmpeg 可执行路径（`None` 时从 PATH 查找）
/// - `interval_secs`：期望的抽帧间隔
/// - `duration_secs`：视频时长（用于估算帧数、必要时拉大间隔）；`None` 时用期望间隔
pub async fn extract_frames(
    video: &Path,
    ffmpeg: Option<&Path>,
    interval_secs: u32,
    duration_secs: Option<f64>,
) -> Result<FrameSet> {
    let ffmpeg_bin = ffmpeg
        .map(|p| p.to_path_buf())
        .or_else(find_ffmpeg)
        .ok_or_else(|| Error::Crawler("ffmpeg 未找到，视觉分析需要 ffmpeg".into()))?;

    let interval = effective_interval(interval_secs, duration_secs);

    let tmp = tempfile::TempDir::new().map_err(|e| Error::Crawler(format!("tempdir: {e}")))?;
    let pattern = tmp.path().join("frame_%04d.jpg");

    // fps=1/interval：每 interval 秒输出一帧；qscale 2-4 是较好画质
    let mut cmd = tokio::process::Command::new(&ffmpeg_bin);
    cmd.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-i",
        &video.to_string_lossy(),
        "-vf",
        &format!("fps=1/{interval},scale=768:-1"),
        "-qscale:v",
        "4",
    ])
    .arg(&pattern)
    .stdout(Stdio::null())
    .stderr(Stdio::piped());

    let output = cmd
        .output()
        .await
        .map_err(|e| Error::Crawler(format!("运行 ffmpeg 失败: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Crawler(format!(
            "ffmpeg 抽帧失败: {}",
            stderr.lines().last().unwrap_or("unknown")
        )));
    }

    let images = collect_frames(tmp.path(), MAX_FRAMES)?;
    if images.is_empty() {
        return Err(Error::Crawler(
            "抽帧结果为空（视频可能无画面或解码失败）".into(),
        ));
    }

    Ok(FrameSet {
        images,
        interval_secs: interval,
    })
}

/// 根据视频时长和帧数上限，算出实际抽帧间隔。
fn effective_interval(desired: u32, duration_secs: Option<f64>) -> u32 {
    let desired = desired.max(1);
    match duration_secs {
        Some(dur) if dur > 0.0 => {
            let frames_at_desired = (dur / desired as f64).ceil() as usize;
            if frames_at_desired <= MAX_FRAMES {
                desired
            } else {
                // 拉大间隔让帧数落在 MAX_FRAMES 内
                (dur / MAX_FRAMES as f64).ceil() as u32
            }
        }
        _ => desired,
    }
}

/// 读取抽出的帧文件，base64 编码；超过 `max` 时均匀降采样。
fn collect_frames(dir: &Path, max: usize) -> Result<Vec<ImageInput>> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| Error::Crawler(format!("读取帧目录: {e}")))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("jpg"))
        .collect();
    paths.sort();

    // 兜底降采样（间隔估算不准或时长未知时仍可能超上限）
    let selected = downsample(&paths, max);

    let mut images = Vec::with_capacity(selected.len());
    for p in selected {
        let bytes = std::fs::read(p).map_err(|e| Error::Crawler(format!("读取帧: {e}")))?;
        let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
        images.push(ImageInput {
            media_type: "image/jpeg".to_string(),
            base64_data: data,
        });
    }
    Ok(images)
}

/// 均匀降采样到最多 `max` 个，保留首尾。
fn downsample(paths: &[PathBuf], max: usize) -> Vec<&PathBuf> {
    if paths.len() <= max || max == 0 {
        return paths.iter().collect();
    }
    let step = paths.len() as f64 / max as f64;
    (0..max)
        .map(|i| &paths[((i as f64) * step) as usize])
        .collect()
}

/// 从 PATH / 常见位置查找 ffmpeg。
fn find_ffmpeg() -> Option<PathBuf> {
    let names = if cfg!(windows) {
        ["ffmpeg.exe", "ffmpeg"].as_slice()
    } else {
        ["ffmpeg"].as_slice()
    };
    for name in names {
        if let Some(paths) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&paths) {
                let p = dir.join(name);
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }
    for c in ["/usr/bin/ffmpeg", "/usr/local/bin/ffmpeg"] {
        if Path::new(c).exists() {
            return Some(PathBuf::from(c));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_respects_frame_cap() {
        // 10 分钟视频，期望每 30 秒 → 20 帧，未超上限
        assert_eq!(effective_interval(30, Some(600.0)), 30);
        // 1 小时视频，每 30 秒 → 120 帧，超上限 → 拉大间隔
        let iv = effective_interval(30, Some(3600.0));
        assert!(iv > 30, "should widen interval, got {iv}");
        assert!((3600.0 / iv as f64).ceil() as usize <= MAX_FRAMES);
        // 时长未知 → 用期望值
        assert_eq!(effective_interval(30, None), 30);
        // 0 间隔被钳到 1
        assert_eq!(effective_interval(0, None), 1);
    }

    #[test]
    fn downsample_keeps_within_cap() {
        let paths: Vec<PathBuf> = (0..100)
            .map(|i| PathBuf::from(format!("f{i:03}.jpg")))
            .collect();
        let picked = downsample(&paths, 10);
        assert_eq!(picked.len(), 10);
        assert_eq!(picked[0], &paths[0]);
        // 均匀分布，最后一个接近末尾
        assert!(picked.last().unwrap().to_string_lossy().contains("f09"));
    }

    #[test]
    fn downsample_noop_under_cap() {
        let paths: Vec<PathBuf> = (0..5).map(|i| PathBuf::from(format!("f{i}.jpg"))).collect();
        assert_eq!(downsample(&paths, 40).len(), 5);
    }

    /// 用 ffmpeg 生成测试视频并真实抽帧（ffmpeg 不可用时跳过）。
    #[tokio::test]
    async fn extract_frames_from_synthetic_video() {
        let Some(ffmpeg) = find_ffmpeg() else {
            eprintln!("ffmpeg 不可用，跳过");
            return;
        };
        let tmp = tempfile::TempDir::new().unwrap();
        let video = tmp.path().join("test.mp4");
        // 90 秒测试视频
        let status = std::process::Command::new(&ffmpeg)
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=90:size=320x240:rate=5",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&video)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());

        let frames = extract_frames(&video, Some(&ffmpeg), 30, Some(90.0))
            .await
            .unwrap();
        // 90 秒 / 30 = 3 帧
        assert_eq!(frames.images.len(), 3);
        assert_eq!(frames.interval_secs, 30);
        for img in &frames.images {
            assert_eq!(img.media_type, "image/jpeg");
            assert!(!img.base64_data.is_empty());
            // base64 能解码回 JPEG 魔数 FF D8
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&img.base64_data)
                .unwrap();
            assert_eq!(&bytes[..2], &[0xFF, 0xD8], "JPEG magic");
        }
    }
}
