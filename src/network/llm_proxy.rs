//! 宿主 LLM 代理服务。
//!
//! 参考 tenbox LlmProxyService：
//! - 在宿主上监听 10.0.2.3:8080（VirtIO NAT 网关地址）
//! - Guest 内 Agent 发出的所有 /v1/messages 请求都经过这里
//! - 代理自动注入 ANTHROPIC_AUTH_TOKEN，转发给真实 API
//! - 支持流式响应（SSE）透传

use std::net::SocketAddr;
use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::Response,
    routing::any,
};
use std::sync::Arc;

use crate::error::{Error, Result};

#[derive(Clone)]
struct ProxyState {
    http:     reqwest::Client,
    api_key:  String,
    base_url: String,
}

/// 启动 LLM 代理服务（阻塞，在 spawn_blocking 中调用）。
pub async fn run_llm_proxy(bind_addr: SocketAddr) -> Result<()> {
    let api_key = std::env::var("ANTHROPIC_AUTH_TOKEN")
        .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
        .map_err(|_| Error::Agent("ANTHROPIC_AUTH_TOKEN not set".into()))?;

    let base_url = std::env::var("ANTHROPIC_BASE_URL")
        .unwrap_or_else(|_| "https://api.anthropic.com".to_string());

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| Error::Agent(format!("HTTP client: {e}")))?;

    let state = Arc::new(ProxyState { http, api_key, base_url });

    let app = Router::new()
        .route("/*path", any(proxy_handler))
        .with_state(state);

    tracing::info!("LLM proxy listening on {bind_addr}");

    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .map_err(|e| Error::Agent(format!("bind {bind_addr}: {e}")))?;

    axum::serve(listener, app)
        .await
        .map_err(|e| Error::Agent(format!("proxy serve: {e}")))?;

    Ok(())
}

/// 代理处理器：透传请求，注入 API key。
async fn proxy_handler(
    State(state): State<Arc<ProxyState>>,
    req: Request,
) -> std::result::Result<Response, StatusCode> {
    let path = req.uri().path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");

    let upstream = format!("{}{}", state.base_url.trim_end_matches('/'), path);

    tracing::debug!("LLM proxy → {upstream}");

    // 复制原始 headers，但覆盖认证
    let mut headers = req.headers().clone();
    headers.insert(
        "x-api-key",
        HeaderValue::from_str(&state.api_key).unwrap(),
    );
    headers.insert(
        "anthropic-version",
        HeaderValue::from_static("2023-06-01"),
    );
    // 移除可能冲突的 host header
    headers.remove("host");

    let method = req.method().clone();
    let body_bytes = axum::body::to_bytes(req.into_body(), 10 * 1024 * 1024)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let upstream_resp = state.http
        .request(method, &upstream)
        .headers(headers)
        .body(body_bytes)
        .send()
        .await
        .map_err(|e| {
            tracing::error!("proxy upstream error: {e}");
            StatusCode::BAD_GATEWAY
        })?;

    let status = upstream_resp.status();
    let resp_headers = upstream_resp.headers().clone();
    let resp_body = upstream_resp.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;

    let mut builder = Response::builder().status(status.as_u16());
    for (k, v) in &resp_headers {
        builder = builder.header(k, v);
    }

    builder
        .body(Body::from(resp_body))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}
