//! HTTP API + Web GUI。

use std::net::SocketAddr;
use std::path::PathBuf;

use axum::{
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

use crate::cli::system::SystemInfo;
use crate::error::{Error, Result};

/// 构建完整应用路由（API + 静态 GUI 资源）。
pub fn build_app() -> Result<Router> {
    let api = Router::new()
        .route("/api/system/info", get(system_info))
        .route("/api/doctor", get(doctor))
        .route("/api/summarize", post(summarize));

    let gui_dir = resolve_gui_dir();
    tracing::debug!("GUI assets: {}", gui_dir.display());

    let index = gui_dir.join("index.html");
    let static_files = ServeDir::new(gui_dir).not_found_service(ServeFile::new(index));

    Ok(Router::new()
        .merge(api)
        .fallback_service(static_files)
        .layer(CorsLayer::permissive()))
}

/// 在随机本地端口启动后台服务（仅供桌面窗口内嵌，不对外暴露）。
pub fn spawn_local_server() -> Result<(u16, std::thread::JoinHandle<()>)> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| Error::Config(format!("bind local port: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| Error::Config(format!("local addr: {e}")))?
        .port();
    listener
        .set_nonblocking(true)
        .map_err(|e| Error::Config(format!("set_nonblocking: {e}")))?;

    let app = build_app()?;

    let handle = std::thread::Builder::new()
        .name("vsb-gui-server".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            rt.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener).expect("tokio listener");
                if let Err(e) = axum::serve(listener, app).await {
                    tracing::error!("gui server error: {e}");
                }
            });
        })
        .map_err(|e| Error::Config(format!("spawn gui server: {e}")))?;

    Ok((port, handle))
}

/// 启动 HTTP 服务（调试 / 远程访问用，桌面应用请用 `vsb gui`）。
pub async fn serve(bind: &str, port: u16) -> Result<()> {
    let addr: SocketAddr = format!("{bind}:{port}")
        .parse()
        .map_err(|e| Error::Config(format!("invalid bind address: {e}")))?;

    let app = build_app()?;

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| Error::Config(format!("bind {addr}: {e}")))?;

    tracing::info!("VSB HTTP server → http://{bind}:{port}");
    axum::serve(listener, app)
        .await
        .map_err(|e| Error::Config(format!("http serve: {e}")))?;
    Ok(())
}

fn resolve_gui_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("VSB_GUI_DIR") {
        return PathBuf::from(dir);
    }

    let mut candidates = Vec::new();
    // 安装目录：与 .exe 同级的 gui/web（Windows 安装程序布局）
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("gui/web"));
        }
    }
    candidates.push(PathBuf::from("gui/web"));
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("gui/web"));

    for c in &candidates {
        if c.join("index.html").exists() {
            return c.clone();
        }
    }
    candidates
        .last()
        .cloned()
        .unwrap_or_else(|| PathBuf::from("gui/web"))
}

#[derive(Serialize)]
struct DoctorResponse {
    required_ok: usize,
    required_total: usize,
    checks: Vec<DoctorCheck>,
}

#[derive(Serialize)]
struct DoctorCheck {
    name: String,
    ok: bool,
    optional: bool,
    detail: String,
}

async fn system_info() -> Json<SystemInfo> {
    Json(SystemInfo::gather())
}

async fn doctor() -> Json<DoctorResponse> {
    let claude = crate::agents::claude::probe();
    let ytdlp = probe_ytdlp_short();
    let ffmpeg = crate::utils::binary::probe_named("ffmpeg");

    let checks = vec![
        DoctorCheck {
            name: "Claude API".into(),
            ok: claude.is_ok(),
            optional: false,
            detail: claude.to_string(),
        },
        DoctorCheck {
            name: "yt-dlp".into(),
            ok: ytdlp.is_ok(),
            optional: false,
            detail: ytdlp.to_string(),
        },
        DoctorCheck {
            name: "FFmpeg (视觉分析)".into(),
            ok: ffmpeg.is_ok(),
            optional: true,
            detail: ffmpeg.to_string(),
        },
    ];

    let required_ok = checks.iter().filter(|c| !c.optional && c.ok).count();
    let required_total = checks.iter().filter(|c| !c.optional).count();

    Json(DoctorResponse {
        required_ok,
        required_total,
        checks,
    })
}

fn probe_ytdlp_short() -> crate::utils::probe::ProbeResult {
    use crate::utils::probe::ProbeResult;
    let names = if cfg!(windows) {
        vec!["yt-dlp.exe", "yt-dlp"]
    } else {
        vec!["yt-dlp"]
    };
    for name in &names {
        if let Some(paths) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&paths) {
                if dir.join(name).exists() {
                    return ProbeResult::ok("yt-dlp");
                }
            }
        }
    }
    ProbeResult::err("yt-dlp", "not installed")
}

async fn summarize(
    Json(req): Json<crate::cli::summary::SummarizeApiRequest>,
) -> ApiResult<Json<crate::cli::summary::SummarizeApiResponse>> {
    Ok(Json(crate::cli::summary::summarize_api(req).await?))
}

type ApiResult<T> = std::result::Result<T, ApiError>;

struct ApiError(Error);

impl From<Error> for ApiError {
    fn from(e: Error) -> Self {
        ApiError(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let msg = self.0.to_string();
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": msg })),
        )
            .into_response()
    }
}
