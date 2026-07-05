//! Claude Code 集成。
//!
//! 通过 Claude Code CLI 或 Anthropic API 与 Claude 模型通信。

use crate::error::Result;
use crate::hypervisor::ProbeResult;

/// Claude Code 探测结果。
pub fn probe() -> ProbeResult {
    // 真实实现：检查 `claude` 命令是否可用
    // 1. 检查 $PATH 中的 claude 可执行文件
    // 2. 检查 ANTHROPIC_API_KEY 环境变量
    // 3. 可选：调用 claude --version

    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        ProbeResult::ok("Claude Code (API key configured)")
    } else if which_claude().is_some() {
        ProbeResult::ok("Claude Code (CLI available)")
    } else {
        ProbeResult::err("Claude Code", "ANTHROPIC_API_KEY not set and claude CLI not found")
    }
}

fn which_claude() -> Option<std::path::PathBuf> {
    // 简化：从 PATH 中查找
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(if cfg!(windows) { "claude.exe" } else { "claude" });
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// Claude 客户端。
pub struct ClaudeClient {
    api_key: String,
    model: String,
    base_url: String,
}

impl ClaudeClient {
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| crate::error::Error::Agent("ANTHROPIC_API_KEY not set".into()))?;
        let model = std::env::var("LLM_MODEL")
            .unwrap_or_else(|_| "claude-sonnet-5".to_string());
        let base_url = std::env::var("ANTHROPIC_BASE_URL")
            .unwrap_or_else(|_| "https://api.anthropic.com".to_string());
        Ok(Self {
            api_key,
            model,
            base_url,
        })
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    /// 发送消息并获取响应（占位）。
    pub async fn send_message(&self, _prompt: &str) -> Result<String> {
        // TODO: 实现 Anthropic API 调用
        Err(crate::error::Error::Agent(
            "send_message not implemented yet".into(),
        ))
    }
}