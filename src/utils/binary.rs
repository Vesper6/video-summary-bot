//! 探测外部二进制命令是否可用。

use crate::hypervisor::ProbeResult;

/// 探测命令是否在 PATH 中（返回通用标签）。
pub fn probe(cmd: &str) -> ProbeResult {
    probe_named(cmd)
}

/// 探测命令是否在 PATH 中（在结果中显示命令名）。
pub fn probe_named(cmd: &str) -> ProbeResult {
    let exe_name = if cfg!(windows) {
        format!("{cmd}.exe")
    } else {
        cmd.to_string()
    };

    // 从 PATH 查找
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            if dir.join(&exe_name).exists() {
                let name: &'static str = Box::leak(cmd.to_string().into_boxed_str());
                return ProbeResult::ok(name);
            }
        }
    }

    // 补充常见安装路径（Windows）
    let extra = [
        "D:/software/ffmpeg-8.1-full_build/bin",
        "D:/software/EVCapture",
        "D:/software/python/3115/Scripts",
        "/usr/local/bin",
        "/usr/bin",
    ];
    for dir in extra {
        let p = std::path::Path::new(dir).join(&exe_name);
        if p.exists() {
            let name: &'static str = Box::leak(cmd.to_string().into_boxed_str());
            return ProbeResult::ok(name);
        }
    }

    ProbeResult::err("not found", "not found in PATH or common locations")
}

