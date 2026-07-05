//! 语音转文字（Whisper）。
//!
//! 通过 FFmpeg 抽取音频，再用 Whisper 转写。

use std::path::Path;

use crate::error::Result;

/// ASR 转写器（占位实现）。
pub struct AsrTranscriber {
    pub model: String, // tiny / base / small / medium / large
}

impl AsrTranscriber {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
        }
    }

    /// 转写视频中的语音。
    pub async fn transcribe(&self, _video_path: &Path) -> Result<String> {
        tracing::warn!("ASR transcriber: not yet implemented");
        Ok(String::new())
    }
}