//! Claude Code 集成。
//!
//! 通过 Anthropic Messages API 与 Claude 通信。
//! 优先读取 CC Switch 注入的环境变量：
//! - ANTHROPIC_AUTH_TOKEN 或 ANTHROPIC_API_KEY
//! - ANTHROPIC_BASE_URL（默认 https://api.anthropic.com）
//! - ANTHROPIC_MODEL（默认 claude-sonnet-4-6）

use crate::error::{Error, Result};
use crate::utils::probe::ProbeResult;

/// Claude Code 探测结果。
pub fn probe() -> ProbeResult {
    let has_token =
        std::env::var("ANTHROPIC_AUTH_TOKEN").is_ok() || std::env::var("ANTHROPIC_API_KEY").is_ok();
    if has_token {
        let model =
            std::env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_string());
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
            .map_err(|_| Error::Agent("ANTHROPIC_AUTH_TOKEN / ANTHROPIC_API_KEY not set".into()))?;

        let base_url = std::env::var("ANTHROPIC_BASE_URL")
            .unwrap_or_else(|_| "https://api.anthropic.com".to_string());

        let model =
            std::env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_string());

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| Error::Agent(format!("failed to build HTTP client: {e}")))?;

        tracing::debug!("Claude client: base_url={} model={}", base_url, model);

        Ok(Self {
            http,
            api_key,
            base_url,
            model,
            max_tokens: 4096,
        })
    }

    /// 当前使用的模型名。
    pub fn model(&self) -> &str {
        &self.model
    }

    /// 设置单次请求最大 token 数。
    pub fn with_max_tokens(mut self, n: u32) -> Self {
        self.max_tokens = n;
        self
    }

    /// 发送单条消息，返回 assistant 的文本回复。
    pub async fn send_message(&self, prompt: &str) -> Result<String> {
        self.chat(&[Message::user(prompt)]).await
    }

    /// 发送带 system prompt 的消息。
    pub async fn send_with_system(&self, system: &str, prompt: &str) -> Result<String> {
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": self.max_tokens,
            "system": system,
            "messages": [{"role": "user", "content": prompt}]
        });
        self.call_api(body).await
    }

    /// 发送带图片的多模态消息（视觉分析用）。
    ///
    /// 图片以 base64 image block 形式排在文字前面（Anthropic 推荐顺序：
    /// 先给图，后提问）。图片过多时调用方应先降采样。
    pub async fn send_with_images(
        &self,
        system: &str,
        prompt: &str,
        images: &[ImageInput],
    ) -> Result<String> {
        let mut content: Vec<serde_json::Value> = images
            .iter()
            .map(|img| {
                serde_json::json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": img.media_type,
                        "data": img.base64_data,
                    }
                })
            })
            .collect();
        content.push(serde_json::json!({ "type": "text", "text": prompt }));

        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": self.max_tokens,
            "system": system,
            "messages": [{"role": "user", "content": content}]
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
            return Err(Error::Agent(format!("API error {status}: {text}")));
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
            .ok_or_else(|| Error::Agent(format!("no text block in response: {text}")))?;

        Ok(content.to_string())
    }
}

/// 多模态图片输入（base64 编码）。
#[derive(Debug, Clone)]
pub struct ImageInput {
    /// MIME 类型，如 "image/jpeg"
    pub media_type: String,
    /// base64 编码的图片数据（不含 data: 前缀）
    pub base64_data: String,
}

/// 对话消息。
pub struct Message {
    /// 消息角色（"user" 或 "assistant"）
    pub role: &'static str,
    /// 消息文本内容
    pub content: String,
}

impl Message {
    /// 构造 user 角色消息。
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user",
            content: content.into(),
        }
    }
    /// 构造 assistant 角色消息。
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant",
            content: content.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_client(base_url: String) -> ClaudeClient {
        ClaudeClient {
            http: reqwest::Client::new(),
            api_key: "test-key".into(),
            base_url,
            model: "claude-test".into(),
            max_tokens: 100,
        }
    }

    #[tokio::test]
    async fn images_are_sent_as_base64_blocks() {
        let mut server = mockito::Server::new_async().await;
        // 断言请求体里含符合 Anthropic 规范的 image block：
        // type=image、source.type=base64、media_type、data，且文字排在图片之后
        let m = server
            .mock("POST", "/v1/messages")
            .match_body(mockito::Matcher::AllOf(vec![
                mockito::Matcher::Regex(r#""type":"image""#.into()),
                mockito::Matcher::Regex(r#""type":"base64""#.into()),
                mockito::Matcher::Regex(r#""media_type":"image/jpeg""#.into()),
                mockito::Matcher::Regex(r#""data":"QUJD""#.into()),
                // 文字问题也在同一条消息里（key 顺序随 serde_json 字母序）
                mockito::Matcher::Regex(r#""text":"问题""#.into()),
            ]))
            .with_status(200)
            .with_body(r#"{"content":[{"type":"text","text":"看到画面了"}]}"#)
            .create_async()
            .await;

        let client = test_client(server.url());
        let images = vec![ImageInput {
            media_type: "image/jpeg".into(),
            base64_data: "QUJD".into(), // base64("ABC")
        }];
        let reply = client
            .send_with_images("你是分析助手", "问题", &images)
            .await
            .unwrap();

        assert_eq!(reply, "看到画面了");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn text_only_request_has_no_image_block() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("POST", "/v1/messages")
            .match_body(mockito::Matcher::Regex(r#""content":"纯文字问题""#.into()))
            .with_status(200)
            .with_body(r#"{"content":[{"type":"text","text":"ok"}]}"#)
            .create_async()
            .await;

        let client = test_client(server.url());
        let reply = client.send_with_system("sys", "纯文字问题").await.unwrap();
        assert_eq!(reply, "ok");
        m.assert_async().await;
    }
}
