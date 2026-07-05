//! 串口控制台设备。
//!
//! 把客户机串口输出转发到 stdout 或文件。

use std::sync::Mutex;

/// 串口设备。
pub struct SerialPort {
    /// 输出 buffer
    buffer: Mutex<Vec<u8>>,
}

impl SerialPort {
    pub fn new() -> Self {
        Self {
            buffer: Mutex::new(Vec::new()),
        }
    }

    /// 客户机写入数据。
    pub fn write(&self, data: &[u8]) {
        // 简化：输出到 stdout + 保存到 buffer
        print!("{}", String::from_utf8_lossy(data));
        let mut buf = self.buffer.lock().expect("serial buffer poisoned");
        buf.extend_from_slice(data);
    }

    /// 客户机读取数据（通常用于 console 输入）。
    pub fn read(&self, _buf: &mut [u8]) -> usize {
        // 简化：从 stdin 读取
        use std::io::Read;
        let stdin = std::io::stdin();
        stdin.lock().read(_buf).unwrap_or(0)
    }

    /// 获取当前 buffer 的快照。
    pub fn buffer_snapshot(&self) -> Vec<u8> {
        self.buffer.lock().expect("serial buffer poisoned").clone()
    }
}

impl Default for SerialPort {
    fn default() -> Self {
        Self::new()
    }
}