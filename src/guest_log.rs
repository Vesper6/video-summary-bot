//! Guest 串口日志钩子：把 vCPU 串口输出的整行日志转发给 GUI / API。
//!
//! 全局钩子模式（与 [`crate::guest_control`] 对应）：
//! - `run_boot` / API 启动流程注册回调（把行写入控制台缓冲）
//! - hypervisor 后端串口设备每凑满一行调用 [`emit`]
//! - VM 退出后清除钩子

use std::sync::{Arc, RwLock};

/// 日志行回调类型（参数为不含换行符的单行文本）。
pub type LogHook = Arc<dyn Fn(&str) + Send + Sync>;

static HOOK: RwLock<Option<LogHook>> = RwLock::new(None);

/// 注册日志回调（VM 启动期间设置，结束后传 `None` 清除）。
pub fn set_hook(hook: Option<LogHook>) {
    if let Ok(mut g) = HOOK.write() {
        *g = hook;
    }
}

/// 由串口设备调用，向已注册的回调转发一行 guest 日志。
/// 未注册钩子时静默丢弃（CLI 路径仍通过 `tracing` 输出）。
pub fn emit(line: &str) {
    if let Ok(guard) = HOOK.read() {
        if let Some(hook) = guard.as_ref() {
            hook(line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn emit_without_hook_is_noop() {
        set_hook(None);
        emit("no hook installed");
    }

    #[test]
    fn emit_forwards_to_hook() {
        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&captured);
        set_hook(Some(Arc::new(move |line: &str| {
            sink.lock().unwrap().push(line.to_string());
        })));
        emit("[guest] hello");
        set_hook(None);
        emit("[guest] dropped");
        let lines = captured.lock().unwrap();
        assert_eq!(lines.as_slice(), ["[guest] hello"]);
    }
}
