//! 探测外部二进制命令是否可用。

use crate::hypervisor::ProbeResult;

/// 探测命令是否在 PATH 中。
pub fn probe(cmd: &str) -> ProbeResult {
    let exe_name = if cfg!(windows) {
        format!("{cmd}.exe")
    } else {
        cmd.to_string()
    };

    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join(&exe_name);
            if candidate.exists() {
                // ProbeResult 内部存 &'static str，cmd 是 &str（短生命周期）
                // 把 cmd 静态化处理：探测成功时使用通用消息
                let _ = cmd;
                return ProbeResult::ok("external binary");
            }
        }
    }
    ProbeResult::err("external binary", "not found in PATH")
}