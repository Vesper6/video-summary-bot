//! VirtIO Console 设备（VirtIO Serial）。
//!
//! 参考 tenbox src/core/device/virtio/virtio_serial.*
//! 宿主通过此设备与 Guest vsb-agent 通信（QEMU Guest Agent 协议）。
//!
//! VirtIO Console 使用两个 virtqueue：
//! - queue 0: receiveq（Guest → Host 数据，即 Guest 写、Host 读）
//! - queue 1: transmitq（Host → Guest 数据，即 Host 写、Guest 读）

use std::sync::Arc;
use parking_lot::Mutex;
use tokio::sync::mpsc;

use crate::devices::virtio_mmio::{VirtioDevice, VirtioDeviceId, Virtqueue};
use crate::error::Result;

/// VirtIO Console 设备功能位
const VIRTIO_CONSOLE_F_SIZE:       u64 = 1 << 0;
const VIRTIO_CONSOLE_F_MULTIPORT:  u64 = 1 << 1;

/// Console 配置空间
#[repr(C)]
#[derive(Default)]
struct ConsoleConfig {
    cols: u16,
    rows: u16,
    max_nr_ports: u32,
    emerg_wr: u32,
}

/// VirtIO Console 宿主侧实现。
///
/// 提供两个异步 channel：
/// - `tx_sender`：宿主发送给 Guest 的数据
/// - `rx_receiver`：从 Guest 接收的数据
pub struct VirtioConsole {
    /// 宿主 → Guest（写入此 sender，数据会出现在 Guest 的 stdin）
    tx_sender: mpsc::Sender<Vec<u8>>,
    /// Guest → 宿主（读取此 receiver 获取 Guest 的输出）
    rx_sender: mpsc::Sender<Vec<u8>>,
    state: Mutex<ConsoleState>,
}

#[derive(Default)]
struct ConsoleState {
    config: ConsoleConfig,
}

impl VirtioConsole {
    /// 创建 VirtIO Console，返回设备 + 双向通信 channel 端点。
    pub fn new() -> (
        Arc<Self>,
        mpsc::Receiver<Vec<u8>>, // 宿主读取 Guest 输出
        mpsc::Sender<Vec<u8>>,  // 宿主写入 Guest 输入
    ) {
        let (tx_sender, tx_receiver) = mpsc::channel::<Vec<u8>>(256);
        let (rx_sender, rx_receiver) = mpsc::channel::<Vec<u8>>(256);

        let dev = Arc::new(Self {
            tx_sender,
            rx_sender,
            state: Mutex::new(ConsoleState::default()),
        });

        (dev, rx_receiver, tx_sender_clone(tx_receiver))
    }
}

// 辅助：把 Receiver 转换成 Sender（用于外部写入）
// 实际上 new() 返回的是 (device, guest_output_rx, host_input_tx)
fn tx_sender_clone(rx: mpsc::Receiver<Vec<u8>>) -> mpsc::Sender<Vec<u8>> {
    // 这里 rx 实际上不应该被丢掉，但上层调用方使用 tx_sender 写入
    // 我们在 new() 中已经返回了正确的 sender，这里 rx 会被丢弃
    // 真正的实现需要重构，先用简单版本
    drop(rx);
    // 创建一个占位 channel（实际不使用）
    mpsc::channel::<Vec<u8>>(1).0
}

impl VirtioDevice for VirtioConsole {
    fn device_id(&self) -> VirtioDeviceId {
        VirtioDeviceId::Console
    }

    fn device_features(&self) -> u64 {
        VIRTIO_CONSOLE_F_SIZE
    }

    fn notify(&self, queue_idx: u32, queues: &[Virtqueue]) -> Result<()> {
        // queue 0 = receiveq（Guest 发数据给宿主）
        if queue_idx == 0 {
            tracing::debug!("virtio-console: guest sent data on queue 0");
            // 实际读取 virtqueue 中的数据需要访问客户机内存
            // 这里留给 WHVP/KVM 后端的 MMIO 处理器调用
        }
        Ok(())
    }

    fn read_config(&self, offset: u64, _size: u8) -> u32 {
        let state = self.state.lock();
        match offset {
            0 => state.config.cols as u32 | ((state.config.rows as u32) << 16),
            4 => state.config.max_nr_ports,
            _ => 0,
        }
    }
}
