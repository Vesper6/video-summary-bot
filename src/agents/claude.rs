//! Claude Code 集成。
//!
//! 通过 Anthropic Messages API 与 Claude 通信。
//! 优先读取 CC Switch 注入的环境变量：
//! - ANTHROPIC_AUTH_TOKEN 或 ANTHROPIC_API_KEY
//! - ANTHROPIC_BASE_URL（默认 https://api.anthropic.com）
//! - ANTHROPIC_MODEL（默认 claude-sonnet-4-6）

use crate::error::{Error, Result};
use crate::hypervisor::ProbeResult;

/// Claude Code 探测结果。
pub fn probe() -> ProbeResult {
    let has_token = std::env::var("ANTHROPIC_AUTH_TOKEN").is_ok()
        || std::env::var("ANTHROPIC_API_KEY").is_ok();
    if has_token {
        let model = std::env::var("ANTHROPIC_MODEL")
            .unwrap_or_else(|_| "claude-sonnet-4-6".to_string());
        // SAFETY: 字符串泄漏仅用于 &'static str，进程生命周期内有效
        let msg: &'static str =
            Box::leak(format!("Claude API ready (model: {model})").into_boxed_str());
        ProbeResult::ok(msg)
    } else {
        ProbeResult::err(
            "Claude Code",
            "ANTHROPIC_AUTH_TOKEN / ANTHROPIC_API_KEY not set",
        )
    }
}

/// Anthropic Messages API 客户端。
pub struct ClaudeClient {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
    model: String,
    max_tokens: u32,
}

impl ClaudeClient {
    /// 从环境变量构建客户端（CC Switch 兼容）。
    pub fn from_env() -> Result<Self> {
        // CC Switch 用 ANTHROPIC_AUTH_TOKEN，标准 SDK 用 ANTHROPIC_API_KEY
        let api_key = std::env::var("ANTHROPIC_AUTH_TOKEN")
            .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
            .map_err(|_| {
                Error::Agent(
                    "ANTHROPIC_AUTH_TOKEN / ANTHROPIC_API_KEY not set".into(),
                )
            })?;

        let base_url = std::env::var("ANTHROPIC_BASE_URL")
            .unwrap_or_else(|_| "https://api.anthropic.com".to_string());

        let model = std::env::var("ANTHROPIC_MODEL")
            .unwrap_or_else(|_| "claude-sonnet-4-6".to_string());

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| Error::Agent(format!("failed to build HTTP client: {e}")))?;

        tracing::debug!(
            "Claude client: base_url={} model={}",
            base_url, model
        );

        Ok(Self {
            http,
            api_key,
            base_url,
            model,
            max_tokens: 4096,
        })
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn with_max_tokens(mut self, n: u32) -> Self {
        self.max_tokens = n;
        self
    }

    /// 发送单条消息，返回 assistant 的文本回复。
    pub async fn send_message(&self, prompt: &str) -> Result<String> {
        self.chat(&[Message::user(prompt)]).await
    }

    /// 发送带 system prompt 的消息。
    pub async fn send_with_system(
        &self,
        system: &str,
        prompt: &str,
    ) -> Result<String> {
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": self.max_tokens,
            "system": system,
            "messages": [{"role": "user", "content": prompt}]
        });
        self.call_api(body).await
    }

    /// 多轮对话接口。
    pub async fn chat(&self, messages: &[Message]) -> Result<String> {
        let msgs: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| serde_json::json!({"role": m.role, "content": m.content}))
            .collect();

        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": self.max_tokens,
            "messages": msgs
        });
        self.call_api(body).await
    }

    /// 底层 HTTP 调用。
    async fn call_api(&self, body: serde_json::Value) -> Result<String> {
        let url = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));

        tracing::debug!("POST {} model={}", url, self.model);

        let resp = self
            .http
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Agent(format!("HTTP request failed: {e}")))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| Error::Agent(format!("failed to read response: {e}")))?;

        if !status.is_success() {
            return Err(Error::Agent(format!(
                "API error {status}: {text}"
            )));
        }

        // 解析 Anthropic Messages API 响应
        // content 是数组，可能含 thinking block，取第一个 type=text 的 block
        let json: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| Error::Agent(format!("invalid JSON response: {e}")))?;

        let content = json["content"]
            .as_array()
            .and_then(|arr| {
                arr.iter()
                    .find(|block| block["type"].as_str() == Some("text"))
                    .and_then(|block| block["text"].as_str())
            })
            .ok_or_else(|| {
                Error::Agent(format!("no text block in response: {text}"))
            })?;

        Ok(content.to_string())
    }
}

/// 对话消息。
pub struct Message {
    pub role: &'static str,
    pub content: String,
}

impl Message {
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user", content: content.into() }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: "assistant", content: content.into() }
    }
}
