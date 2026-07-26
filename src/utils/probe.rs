//! 依赖探测结果类型。
//!
//! 供 doctor / info 等健康检查使用，描述某个外部依赖（yt-dlp、FFmpeg、
//! Claude API、cookies 等）是否可用。

/// 依赖探测结果。
#[derive(Debug, Clone, Copy)]
pub struct ProbeResult {
    /// 是否可用
    pub available: bool,
    /// 后端名称
    pub backend: &'static str,
    /// 不可用时的错误说明
    pub error: Option<&'static str>,
}

impl ProbeResult {
    /// 构造「可用」结果。
    pub fn ok(backend: &'static str) -> Self {
        Self {
            available: true,
            backend,
            error: None,
        }
    }

    /// 构造「不可用」结果。
    pub fn err(backend: &'static str, error: &'static str) -> Self {
        Self {
            available: false,
            backend,
            error: Some(error),
        }
    }

    /// 依赖是否可用。
    pub fn is_ok(&self) -> bool {
        self.available
    }
}

impl std::fmt::Display for ProbeResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.available {
            write!(f, "✓ {} (可用)", self.backend)
        } else {
            write!(
                f,
                "✗ {} (不可用{})",
                self.backend,
                self.error.map(|e| format!(": {e}")).unwrap_or_default()
            )
        }
    }
}
