//! 宿主 ↔ Guest 通信：基于 VirtIO Serial 的精简 QEMU Guest Agent 协议。
//!
//! 参考 tenbox src/core/guest_agent/guest_agent.cpp：
//! 1. 发送 0xFF 字节重置 Guest 缓冲区
//! 2. 发送 guest-sync-delimited，等待 Guest 回复同步 ID
//! 3. 后续发送任意命令（JSON），读取 {"return": ...} 响应

use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};

/// QEMU GA 命令。
#[derive(Debug, Serialize)]
pub struct GaCommand {
    pub execute: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
}

/// QEMU GA 响应。
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum GaResponse {
    Ok    { r#return: Value },
    Error { error: GaError },
}

#[derive(Debug, Deserialize)]
pub struct GaError {
    pub class: String,
    pub desc:  String,
}

/// VirtIO Serial 上的 Guest Agent 客户端（宿主侧）。
pub struct GuestAgentClient {
    /// VirtIO Serial 字符设备句柄（宿主侧映射的文件描述符）
    /// 实际上是 VMM 通过 VirtIO Console 暴露给宿主的 FIFO / socket
    serial_path: std::path::PathBuf,
    timeout:     Duration,
}

impl GuestAgentClient {
    pub fn new(serial_path: std::path::PathBuf) -> Self {
        Self {
            serial_path,
            timeout: Duration::from_secs(30),
        }
    }

    pub fn with_timeout(mut self, t: Duration) -> Self {
        self.timeout = t;
        self
    }

    /// 建立连接并完成握手（guest-sync-delimited）。
    pub async fn connect(&self) -> Result<GuestAgentSession> {
        let sync_id: u64 = rand_sync_id();

        // 打开 serial 设备（宿主侧是一个 Unix socket 或 named pipe）
        #[cfg(unix)]
        let mut stream = {
            use tokio::net::UnixStream;
            tokio::time::timeout(
                self.timeout,
                UnixStream::connect(&self.serial_path),
            )
            .await
            .map_err(|_| Error::Ipc("connect timeout".into()))?
            .map_err(|e| Error::Ipc(format!("connect failed: {e}")))?
        };

        #[cfg(windows)]
        let mut stream = {
            use tokio::net::windows::named_pipe::ClientOptions;
            ClientOptions::new()
                .open(&self.serial_path)
                .map_err(|e| Error::Ipc(format!("open pipe failed: {e}")))?
        };

        tracing::debug!("GA: connected to {:?}", self.serial_path);

        // 1. 发 0xFF 重置 Guest 缓冲区
        stream.write_all(&[0xFF]).await
            .map_err(|e| Error::Ipc(format!("write 0xFF failed: {e}")))?;

        // 2. guest-sync-delimited
        let sync_cmd = serde_json::to_vec(&GaCommand {
            execute: "guest-sync-delimited",
            arguments: Some(serde_json::json!({ "id": sync_id })),
        })?;
        let mut payload = vec![0xFF_u8];
        payload.extend_from_slice(&sync_cmd);
        payload.push(b'\n');
        stream.write_all(&payload).await
            .map_err(|e| Error::Ipc(format!("write sync failed: {e}")))?;

        // 3. 等待含匹配 ID 的响应
        let resp = read_response_timeout(&mut stream, self.timeout).await?;
        match resp {
            GaResponse::Ok { r#return: v } => {
                let got = v.as_u64().unwrap_or(0);
                if got != sync_id {
                    return Err(Error::Ipc(format!(
                        "sync ID mismatch: expected {sync_id} got {got}"
                    )));
                }
            }
            GaResponse::Error { error } => {
                return Err(Error::Ipc(format!("sync error: {}", error.desc)));
            }
        }

        tracing::info!("GA: handshake OK (sync_id={})", sync_id);
        Ok(GuestAgentSession { stream, timeout: self.timeout })
    }
}

/// 已建立握手的 Guest Agent 会话。
pub struct GuestAgentSession {
    #[cfg(unix)]
    stream: tokio::net::UnixStream,
    #[cfg(windows)]
    stream: tokio::net::windows::named_pipe::NamedPipeClient,
    timeout: Duration,
}

impl GuestAgentSession {
    /// 发送命令，返回 `return` 字段的 JSON 值。
    pub async fn execute(
        &mut self,
        cmd: &'static str,
        args: Option<Value>,
    ) -> Result<Value> {
        let payload = serde_json::to_vec(&GaCommand { execute: cmd, arguments: args })?;
        let mut msg = payload;
        msg.push(b'\n');

        self.stream.write_all(&msg).await
            .map_err(|e| Error::Ipc(format!("write failed: {e}")))?;

        match read_response_timeout(&mut self.stream, self.timeout).await? {
            GaResponse::Ok { r#return: v } => Ok(v),
            GaResponse::Error { error } => {
                Err(Error::Ipc(format!("[{}] {}", error.class, error.desc)))
            }
        }
    }

    /// 向 Guest 发送视频总结任务。
    pub async fn run_summary(
        &mut self,
        url: &str,
        level: &str,
        language: &str,
    ) -> Result<String> {
        let ret = self.execute(
            "vsb-run-summary",
            Some(serde_json::json!({
                "url":      url,
                "level":    level,
                "language": language,
            })),
        ).await?;

        let subtitle = ret["subtitle"].as_str().unwrap_or("").to_string();
        Ok(subtitle)
    }

    /// ping Guest（检活）。
    pub async fn ping(&mut self) -> Result<()> {
        self.execute("guest-ping", None).await?;
        Ok(())
    }

    /// 关闭 Guest。
    pub async fn shutdown(&mut self) -> Result<()> {
        let _ = self.execute(
            "guest-shutdown",
            Some(serde_json::json!({ "mode": "powerdown" })),
        ).await;
        Ok(())
    }
}

// =============================================
// 辅助函数
// =============================================

/// 从流中读取一行 JSON 响应（带超时）。
async fn read_response_timeout<S>(stream: &mut S, timeout: Duration) -> Result<GaResponse>
where
    S: AsyncReadExt + Unpin,
{
    let mut buf = Vec::with_capacity(4096);
    tokio::time::timeout(timeout, async {
        let mut tmp = [0u8; 1];
        loop {
            stream.read_exact(&mut tmp).await
                .map_err(|e| Error::Ipc(format!("read failed: {e}")))?;

            // 跳过 0xFF 哨兵字节
            if tmp[0] == 0xFF {
                buf.clear();
                continue;
            }

            if tmp[0] == b'\n' {
                break;
            }
            buf.push(tmp[0]);
        }
        Ok::<_, Error>(())
    })
    .await
    .map_err(|_| Error::Ipc("read response timeout".into()))??;

    let resp: GaResponse = serde_json::from_slice(&buf)
        .map_err(|e| Error::Ipc(format!("parse response failed: {e}: {:?}", &buf)))?;

    Ok(resp)
}

fn rand_sync_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(12345)
}
