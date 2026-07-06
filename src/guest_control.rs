//! Guest 运行控制钩子：让 API/GUI 的「停止」真正中断 vCPU run 循环。
//!
//! 与 `guest_log` 同样的全局钩子模式：
//! - `run_boot` 启动 vCPU 后注册 stop 回调（调用 `hv.request_stop()`）
//! - API `stop` 调用 `request_stop()` 触发它
//! - vCPU 退出后清除钩子

use std::sync::{Arc, RwLock};

type StopHook = Arc<dyn Fn() + Send + Sync>;

static HOOK: RwLock<Option<StopHook>> = RwLock::new(None);

/// 注册停止回调（VM 启动期间设置，结束后清除）。
pub fn set_hook(hook: Option<StopHook>) {
    if let Ok(mut g) = HOOK.write() {
        *g = hook;
    }
}

/// 由 API/GUI 停止操作调用，请求正在运行的 vCPU 优雅退出。
/// 返回 true 表示确有 VM 在运行并已请求停止。
pub fn request_stop() -> bool {
    if let Ok(guard) = HOOK.read() {
        if let Some(hook) = guard.as_ref() {
            hook();
            return true;
        }
    }
    false
}
