//! HTTP API + Web GUI（参考 tenbox Manager / tenboxd RPC）。

mod vm_store;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
};
use serde::Serialize;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use vm_store::{CreateVmRequest, VmStore, VmSummary};

use crate::cli::system::SystemInfo;
use crate::error::{Error, Result};

/// 共享 API 状态。
#[derive(Clone)]
pub struct ApiState {
    pub vms: Arc<VmStore>,
}

/// 构建完整应用路由（API + 静态 GUI 资源）。
pub fn build_app(vms: Arc<VmStore>) -> Result<Router> {
    let state = ApiState { vms };

    let api = Router::new()
        .route("/api/system/info", get(system_info))
        .route("/api/doctor", get(doctor))
        .route("/api/vms", get(list_vms).post(create_vm))
        .route("/api/vms/:name", get(get_vm).delete(delete_vm))
        .route("/api/vms/:name/start", post(start_vm))
        .route("/api/vms/:name/stop", post(stop_vm))
        .route("/api/vms/:name/reboot", post(reboot_vm))
        .route("/api/vms/:name/shutdown", post(shutdown_vm))
        .with_state(state);

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

    let vms = Arc::new(VmStore::new(VmStore::default_path())?);
    let app = build_app(vms)?;

    let handle = std::thread::Builder::new()
        .name("vsb-gui-server".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            rt.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener)
                    .expect("tokio listener");
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

    let vms = Arc::new(VmStore::new(VmStore::default_path())?);
    let app = build_app(vms)?;

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
    let candidates = [
        PathBuf::from("gui/web"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("gui/web"),
    ];
    for c in &candidates {
        if c.join("index.html").exists() {
            return c.clone();
        }
    }
    candidates[1].clone()
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
    let hv = crate::hypervisor::probe();
    let ffmpeg = crate::utils::binary::probe_named("ffmpeg");
    let assets = crate::utils::resource::probe();

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
            name: "Hypervisor (VM)".into(),
            ok: hv.is_ok(),
            optional: true,
            detail: hv.to_string(),
        },
        DoctorCheck {
            name: "FFmpeg (ASR)".into(),
            ok: ffmpeg.is_ok(),
            optional: true,
            detail: ffmpeg.to_string(),
        },
        DoctorCheck {
            name: "assets/ (VM kernels)".into(),
            ok: assets.is_ok(),
            optional: true,
            detail: assets.to_string(),
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

fn probe_ytdlp_short() -> crate::hypervisor::ProbeResult {
    let names = if cfg!(windows) {
        vec!["yt-dlp.exe", "yt-dlp"]
    } else {
        vec!["yt-dlp"]
    };
    for name in &names {
        if let Some(paths) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&paths) {
                if dir.join(name).exists() {
                    return crate::hypervisor::ProbeResult::ok("yt-dlp");
                }
            }
        }
    }
    crate::hypervisor::ProbeResult::err("yt-dlp", "not installed")
}

async fn list_vms(State(state): State<ApiState>) -> ApiResult<Json<Vec<VmSummary>>> {
    Ok(Json(state.vms.list()?))
}

async fn get_vm(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> ApiResult<Json<VmSummary>> {
    Ok(Json(state.vms.get(&name)?))
}

async fn create_vm(
    State(state): State<ApiState>,
    Json(req): Json<CreateVmRequest>,
) -> ApiResult<Json<VmSummary>> {
    Ok(Json(state.vms.create(req)?))
}

async fn delete_vm(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> ApiResult<StatusCode> {
    state.vms.delete(&name)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn start_vm(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> ApiResult<Json<VmSummary>> {
    Ok(Json(state.vms.start(&name)?))
}

async fn stop_vm(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> ApiResult<Json<VmSummary>> {
    Ok(Json(state.vms.stop(&name)?))
}

async fn reboot_vm(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> ApiResult<Json<VmSummary>> {
    Ok(Json(state.vms.reboot(&name)?))
}

async fn shutdown_vm(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> ApiResult<Json<VmSummary>> {
    Ok(Json(state.vms.shutdown(&name)?))
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
        (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": msg }))).into_response()
    }
}