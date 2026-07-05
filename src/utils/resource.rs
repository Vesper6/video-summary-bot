//! 探测项目资源（内核镜像、initramfs、rootfs）。

use crate::hypervisor::ProbeResult;

/// 默认资源目录。
pub fn resource_dir() -> std::path::PathBuf {
    // 优先从环境变量获取
    if let Ok(p) = std::env::var("VSB_RESOURCE_DIR") {
        return std::path::PathBuf::from(p);
    }

    // 项目根目录的 assets/
    let exe = std::env::current_exe().ok();
    if let Some(exe) = exe {
        // exe 可能在 target/debug/video-summary-bot.exe
        // 期望走到：target/debug → target → 项目根
        let mut p = exe.parent().map(|p| p.to_path_buf());
        if let Some(ref mut p) = p {
            // 跳过 target/debug 或 target/release
            if p.ends_with("debug") || p.ends_with("release") {
                if let Some(parent) = p.parent() {
                    *p = parent.to_path_buf();
                }
            }
            // 跳过 target/（如果存在）
            if p.ends_with("target") {
                if let Some(parent) = p.parent() {
                    *p = parent.to_path_buf();
                }
            }
        }
        if let Some(p) = p {
            return p.join("assets");
        }
    }

    std::path::PathBuf::from("./assets")
}

/// 探测资源完整性。
pub fn probe() -> ProbeResult {
    let dir = resource_dir();
    if !dir.exists() {
        let msg: &'static str = Box::leak(format!("{} does not exist", dir.display()).into_boxed_str());
        return ProbeResult::err("resources", msg);
    }

    let required = ["rootfs", "kernels", "initramfs"];
    for r in &required {
        let sub = dir.join(r);
        if !sub.exists() {
            let msg: &'static str = Box::leak(format!("missing: {}/{}", dir.display(), r).into_boxed_str());
            return ProbeResult::err("resources", msg);
        }
    }

    ProbeResult::ok("resources")
}