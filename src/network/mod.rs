//! 网络模块：NAT、DHCP、LLM 代理。
//!
//! 参考 tenbox：
//! - Guest 通过 VirtIO NIC 访问 NAT 网络（10.0.2.0/24）
//! - 默认网关 10.0.2.2，DNS/代理 10.0.2.3
//! - LLM 代理监听在 10.0.2.3:8080，转发给真实 Anthropic API

pub mod dhcp;
pub mod llm_proxy;
pub mod nat;

use serde::{Deserialize, Serialize};

/// 网络配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub subnet: String,
    pub gateway: String,
    pub llm_proxy_port: u16,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            subnet: "10.0.2.0/24".to_string(),
            gateway: "10.0.2.3".to_string(),
            llm_proxy_port: 8080,
        }
    }
}
