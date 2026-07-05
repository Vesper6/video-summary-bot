//! 内置 DHCP 服务器。

use std::net::Ipv4Addr;

/// DHCP 服务器（占位实现）。
pub struct DhcpServer {
    pub server_ip: Ipv4Addr,
    pub lease_start: Ipv4Addr,
    pub lease_end: Ipv4Addr,
}

impl DhcpServer {
    pub fn new(network: std::net::Ipv4Addr) -> Self {
        Self {
            server_ip: network,
            lease_start: Ipv4Addr::new(10, 0, 0, 100),
            lease_end: Ipv4Addr::new(10, 0, 0, 200),
        }
    }

    /// 处理 DHCP 请求，返回分配的 IP。
    pub fn allocate(&self, _mac: [u8; 6]) -> Option<Ipv4Addr> {
        // TODO: 维护租约表
        Some(self.lease_start)
    }
}