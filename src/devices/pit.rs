//! 8253/8254 PIT（可编程间隔定时器）模拟。
//!
//! Linux 内核启动时通过 PIT 校准 TSC / LAPIC 定时器频率：
//! - `pit_calibrate_tsc()`：通道 2 编程为 mode 0，轮询 port 0x61 bit5（OUT2）
//! - `quick_pit_calibrate()`：通道 2 mode 0，反复读 port 0x42（lo/hi）观察递减
//!
//! 之前 guest 卡在 port 0x42 就是因为读到恒定 0xFF、计数器永不递减。
//!
//! 模拟策略：不做周期性 tick，读计数器时按宿主单调时钟推算
//! 已流逝的 PIT 时钟数（1.193182 MHz），行为对校准代码完全等价。
//!
//! 端口：
//! - `0x40`/`0x41`/`0x42`：通道 0/1/2 数据
//! - `0x43`：命令字（通道选择 + 访问模式 + 工作模式）
//! - `0x61`：NMI 状态/控制（bit0 = 通道 2 gate，bit4 = 刷新翻转，bit5 = OUT2）

use std::time::Instant;

/// PIT 输入时钟频率（Hz）。
pub const PIT_FREQ_HZ: u64 = 1_193_182;

/// 端口 0x61 每 ~15µs 翻转一次的刷新位周期（纳秒）。
const REFRESH_TOGGLE_NS: u64 = 15_000;

/// 数据端口访问模式（命令字 bit5-4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccessMode {
    /// 仅低字节
    LoByte,
    /// 仅高字节
    HiByte,
    /// 低字节后高字节（最常用）
    LoHi,
}

/// 单个 PIT 通道。
#[derive(Debug)]
struct PitChannel {
    /// 重装值（0 表示 65536）
    reload: u32,
    /// 工作模式（命令字 bit3-1）
    mode: u8,
    /// 访问模式
    access: AccessMode,
    /// lo/hi 访问的写状态：下一个写是否高字节
    write_hi_next: bool,
    /// lo/hi 访问的读状态：下一个读是否高字节
    read_hi_next: bool,
    /// 写入一半的低字节（等待高字节）
    pending_lo: u8,
    /// 计数开始时刻（编程完成时间）
    armed_at: Option<Instant>,
    /// 锁存的计数值（latch 命令）
    latched: Option<u16>,
    /// 锁存的状态字节（read-back 命令）
    latched_status: Option<u8>,
}

impl Default for PitChannel {
    fn default() -> Self {
        Self {
            reload: 0x1_0000,
            mode: 2,
            access: AccessMode::LoHi,
            write_hi_next: false,
            read_hi_next: false,
            pending_lo: 0,
            armed_at: None,
            latched: None,
            latched_status: None,
        }
    }
}

impl PitChannel {
    /// 编程后至今流逝的 PIT 时钟数。
    fn elapsed_ticks(&self, now: Instant) -> u64 {
        match self.armed_at {
            Some(t0) => {
                let ns = now.duration_since(t0).as_nanos() as u64;
                ns.saturating_mul(PIT_FREQ_HZ) / 1_000_000_000
            }
            None => 0,
        }
    }

    /// 当前计数值。
    fn current_count(&self, now: Instant) -> u16 {
        let reload = if self.reload == 0 {
            0x1_0000
        } else {
            self.reload
        } as u64;
        let ticks = self.elapsed_ticks(now);
        match self.mode {
            // mode 0：一次性倒数到 0 后停在 0（实际硬件回绕，校准代码只关心到达终点）
            0 | 1 => reload.saturating_sub(ticks) as u16,
            // mode 2/3 等：周期性倒数
            _ => {
                if reload == 0 {
                    0
                } else {
                    (reload - (ticks % reload)) as u16
                }
            }
        }
    }

    /// OUT 引脚电平（mode 0：计数到 0 后变高）。
    fn output(&self, now: Instant) -> bool {
        let reload = if self.reload == 0 {
            0x1_0000
        } else {
            self.reload
        } as u64;
        match self.mode {
            0 => self.elapsed_ticks(now) >= reload,
            // mode 2：仅在计数到 1 时输出一个短低脉冲，其余时间为高
            2 => true,
            // mode 3：方波，前半周期高
            3 => {
                if reload == 0 {
                    true
                } else {
                    (self.elapsed_ticks(now) % reload) < reload / 2
                }
            }
            _ => true,
        }
    }

    /// 状态字节（read-back 命令用）。
    fn status(&self, now: Instant) -> u8 {
        let out = if self.output(now) { 0x80 } else { 0 };
        let access = match self.access {
            AccessMode::LoByte => 1u8,
            AccessMode::HiByte => 2,
            AccessMode::LoHi => 3,
        };
        out | (access << 4) | ((self.mode & 0x7) << 1)
    }
}

/// 8254 PIT + 端口 0x61 模拟。
pub struct Pit {
    channels: [PitChannel; 3],
    /// 端口 0x61 可写位（bit0 = ch2 gate，bit1 = speaker data）
    port61: u8,
    /// 设备创建时刻（0x61 刷新位翻转的时间基准）
    created_at: Instant,
}

impl Pit {
    /// 创建 PIT（所有通道未编程）。
    pub fn new() -> Self {
        Self {
            channels: Default::default(),
            port61: 0,
            created_at: Instant::now(),
        }
    }

    /// 此端口是否由 PIT 处理。
    pub fn handles_port(port: u16) -> bool {
        matches!(port, 0x40..=0x43 | 0x61)
    }

    /// 端口读。
    pub fn io_read(&mut self, port: u16) -> u8 {
        let now = Instant::now();
        match port {
            0x40..=0x42 => self.read_counter((port - 0x40) as usize, now),
            // 0x43 命令端口不可读
            0x43 => 0xFF,
            0x61 => {
                // bit4：~15µs 翻转一次的刷新位（校准代码有时用它做短延时）
                let ns = now.duration_since(self.created_at).as_nanos() as u64;
                let refresh = ((ns / REFRESH_TOGGLE_NS) & 1) as u8;
                // bit5：通道 2 OUT
                let out2 = self.channels[2].output(now) as u8;
                (self.port61 & 0x0F) | (refresh << 4) | (out2 << 5)
            }
            _ => 0xFF,
        }
    }

    /// 端口写。
    pub fn io_write(&mut self, port: u16, value: u8) {
        let now = Instant::now();
        match port {
            0x40..=0x42 => self.write_counter((port - 0x40) as usize, value, now),
            0x43 => self.write_command(value, now),
            0x61 => {
                let old_gate = self.port61 & 1;
                self.port61 = value & 0x0F;
                // gate 0→1：mode 0 重新开始计数（校准代码先开 gate 再编程，
                // 这里同时兼容先编程后开 gate 的顺序）
                if old_gate == 0 && (value & 1) == 1 && self.channels[2].mode == 0 {
                    if let Some(ch) = self.channels.get_mut(2) {
                        if ch.armed_at.is_some() {
                            ch.armed_at = Some(now);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn read_counter(&mut self, idx: usize, now: Instant) -> u8 {
        let ch = &mut self.channels[idx];
        // 状态锁存优先（read-back 命令）
        if let Some(st) = ch.latched_status.take() {
            return st;
        }
        let count = ch.latched.unwrap_or_else(|| ch.current_count(now));
        match ch.access {
            AccessMode::LoByte => {
                ch.latched = None;
                (count & 0xFF) as u8
            }
            AccessMode::HiByte => {
                ch.latched = None;
                (count >> 8) as u8
            }
            AccessMode::LoHi => {
                if ch.read_hi_next {
                    ch.read_hi_next = false;
                    ch.latched = None; // lo/hi 对读完，解除锁存
                    (count >> 8) as u8
                } else {
                    ch.read_hi_next = true;
                    // 锁存以保证 lo/hi 读到同一瞬间的值
                    if ch.latched.is_none() {
                        ch.latched = Some(count);
                    }
                    (count & 0xFF) as u8
                }
            }
        }
    }

    fn write_counter(&mut self, idx: usize, value: u8, now: Instant) {
        let ch = &mut self.channels[idx];
        match ch.access {
            AccessMode::LoByte => {
                ch.reload = value as u32;
                ch.armed_at = Some(now);
            }
            AccessMode::HiByte => {
                ch.reload = (value as u32) << 8;
                ch.armed_at = Some(now);
            }
            AccessMode::LoHi => {
                if ch.write_hi_next {
                    ch.write_hi_next = false;
                    let v = ((value as u32) << 8) | ch.pending_lo as u32;
                    ch.reload = v;
                    ch.armed_at = Some(now);
                } else {
                    ch.pending_lo = value;
                    ch.write_hi_next = true;
                    // 写低字节时暂停计数（硬件行为），简化为重置起点
                    ch.armed_at = None;
                }
            }
        }
    }

    fn write_command(&mut self, cmd: u8, now: Instant) {
        let sel = (cmd >> 6) & 0x3;
        if sel == 3 {
            // read-back 命令：bit5=0 锁存计数，bit4=0 锁存状态，bit3-1 通道掩码
            let latch_count = cmd & 0x20 == 0;
            let latch_status = cmd & 0x10 == 0;
            for (i, ch) in self.channels.iter_mut().enumerate() {
                if cmd & (0x02 << i) != 0 {
                    if latch_count && ch.latched.is_none() {
                        ch.latched = Some(ch.current_count(now));
                        ch.read_hi_next = false;
                    }
                    if latch_status && ch.latched_status.is_none() {
                        ch.latched_status = Some(ch.status(now));
                    }
                }
            }
            return;
        }

        let ch = &mut self.channels[sel as usize];
        let access = (cmd >> 4) & 0x3;
        if access == 0 {
            // 锁存命令：锁存当前计数供 lo/hi 读取
            if ch.latched.is_none() {
                ch.latched = Some(ch.current_count(now));
                ch.read_hi_next = false;
            }
            return;
        }

        ch.access = match access {
            1 => AccessMode::LoByte,
            2 => AccessMode::HiByte,
            _ => AccessMode::LoHi,
        };
        ch.mode = (cmd >> 1) & 0x7;
        ch.write_hi_next = false;
        ch.read_hi_next = false;
        ch.latched = None;
        ch.latched_status = None;
        // 编程后计数器停止，等待写入重装值
        ch.armed_at = None;
    }
}

impl Default for Pit {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    /// 模拟 Linux `pit_calibrate_tsc()` 的通道 2 用法。
    #[test]
    fn channel2_mode0_out_goes_high_at_terminal_count() {
        let mut pit = Pit::new();
        // gate on（speaker off）
        let v = pit.io_read(0x61);
        pit.io_write(0x61, (v & 0xFD) | 0x01);
        // 通道 2，lo/hi，mode 0
        pit.io_write(0x43, 0xB0);
        // 倒数 ~1ms：1193 ticks
        let latch = 1193u16;
        pit.io_write(0x42, (latch & 0xFF) as u8);
        pit.io_write(0x42, (latch >> 8) as u8);

        // 刚编程完 OUT2 应为低
        assert_eq!(
            pit.io_read(0x61) & 0x20,
            0,
            "OUT2 should be low right after arming"
        );

        // 等 2ms 后 OUT2 应变高
        sleep(Duration::from_millis(2));
        assert_ne!(
            pit.io_read(0x61) & 0x20,
            0,
            "OUT2 should be high after terminal count"
        );
    }

    /// 模拟 `quick_pit_calibrate()`：反复读 0x42 应观察到计数递减。
    #[test]
    fn channel2_counter_decrements_over_time() {
        let mut pit = Pit::new();
        pit.io_write(0x61, 0x01);
        pit.io_write(0x43, 0xB0);
        pit.io_write(0x42, 0xFF);
        pit.io_write(0x42, 0xFF); // reload = 0xFFFF

        let read_count = |pit: &mut Pit| -> u16 {
            let lo = pit.io_read(0x42) as u16;
            let hi = pit.io_read(0x42) as u16;
            (hi << 8) | lo
        };

        let c1 = read_count(&mut pit);
        sleep(Duration::from_millis(3));
        let c2 = read_count(&mut pit);
        assert!(c2 < c1, "counter should decrement: {c1} -> {c2}");
        // 3ms ≈ 3579 ticks，允许调度抖动
        let delta = c1 - c2;
        assert!(
            (2000..=20000).contains(&delta),
            "unexpected tick delta {delta}"
        );
    }

    /// latch 命令锁存瞬时值，lo/hi 两次读取应一致。
    #[test]
    fn latch_freezes_count_for_paired_read() {
        let mut pit = Pit::new();
        pit.io_write(0x43, 0x34); // ch0, lo/hi, mode 2
        pit.io_write(0x40, 0x00);
        pit.io_write(0x40, 0x10); // reload = 0x1000

        pit.io_write(0x43, 0x00); // latch ch0
        let lo = pit.io_read(0x40);
        sleep(Duration::from_millis(1));
        let hi = pit.io_read(0x40);
        let latched = ((hi as u16) << 8) | lo as u16;
        assert!(latched <= 0x1000);

        // 再 latch 一次应得到不同（更小或回绕）的值，而两次锁存内部一致
        pit.io_write(0x43, 0x00);
        let lo2 = pit.io_read(0x40);
        let hi2 = pit.io_read(0x40);
        let latched2 = ((hi2 as u16) << 8) | lo2 as u16;
        assert_ne!(
            (lo, hi),
            (lo2, hi2),
            "latched pairs should differ over time: {latched:#x} vs {latched2:#x}"
        );
    }

    #[test]
    fn port61_refresh_bit_toggles() {
        let mut pit = Pit::new();
        // 在 ~2ms 窗口内轮询，收集刷新位的两种取值（周期 15µs，足够翻转多次）
        let mut saw_low = false;
        let mut saw_high = false;
        let deadline = Instant::now() + Duration::from_millis(5);
        while Instant::now() < deadline {
            if pit.io_read(0x61) & 0x10 == 0 {
                saw_low = true;
            } else {
                saw_high = true;
            }
            if saw_low && saw_high {
                break;
            }
        }
        assert!(saw_low && saw_high, "refresh bit should toggle over time");
    }

    #[test]
    fn readback_returns_status_byte() {
        let mut pit = Pit::new();
        pit.io_write(0x43, 0xB0); // ch2 mode 0 lo/hi
        pit.io_write(0x42, 0x10);
        pit.io_write(0x42, 0x00);
        // read-back：锁存 ch2 状态（bit4=0 状态，bit5=1 不锁计数，bit3=ch2）
        pit.io_write(0x43, 0xC0 | 0x20 | 0x08);
        let status = pit.io_read(0x42);
        assert_eq!(status & 0x30, 0x30, "access mode lo/hi");
        assert_eq!((status >> 1) & 0x7, 0, "mode 0");
    }
}
