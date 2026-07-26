//! HTTP API + Web GUI（参考 tenbox Manager / tenboxd RPC）。

pub mod vm_store;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use std::convert::Infallible;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures::stream::Stream;
use serde::Serialize;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use vm_store::{CreateVmRequest, UpdateVmRequest, VmStore, VmSummary};

use crate::cli::system::SystemInfo;
use crate::error::{Error, Result};

/// 共享 API 状态。
#[derive(Clone)]
pub struct ApiState {
    /// VM 存储（共享句柄）
    pub vms: Arc<VmStore>,
}

/// 构建完整应用路由（API + 静态 GUI 资源）。
pub fn build_app(vms: Arc<VmStore>) -> Result<Router> {
    let state = ApiState { vms };

    let api = Router::new()
        .route("/api/system/info", get(system_info))
        .route("/api/doctor", get(doctor))
        .route("/api/vms", get(list_vms).post(create_vm))
        .route(
            "/api/vms/:name",
            get(get_vm).put(update_vm).delete(delete_vm),
        )
        .route("/api/vms/:name/start", post(start_vm))
        .route("/api/vms/:name/stop", post(stop_vm))
        .route("/api/vms/:name/reboot", post(reboot_vm))
        .route("/api/vms/:name/shutdown", post(shutdown_vm))
        .route("/api/vms/:name/console/stream", get(console_stream))
        .route("/api/summarize", post(summarize))
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

async fn update_vm(
    State(state): State<ApiState>,
    Path(name): Path<String>,
    Json(req): Json<UpdateVmRequest>,
) -> ApiResult<Json<VmSummary>> {
    Ok(Json(state.vms.update(&name, req)?))
}

/// SSE 实时控制台流：每行一个 `data:` 事件，新行按 500ms 轮询推送。
async fn console_stream(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> ApiResult<Sse<impl Stream<Item = std::result::Result<Event, Infallible>>>> {
    // 先校验 VM 存在，让 404 类错误走普通 JSON 响应而不是空流
    state.vms.get(&name)?;

    struct StreamState {
        vms: Arc<VmStore>,
        name: String,
        cursor: usize,
        pending: std::collections::VecDeque<String>,
    }

    let init = StreamState {
        vms: state.vms.clone(),
        name,
        cursor: 0,
        pending: std::collections::VecDeque::new(),
    };

    let stream = futures::stream::unfold(init, |mut st| async move {
        loop {
            if let Some(line) = st.pending.pop_front() {
                return Some((Ok(Event::default().data(line)), st));
            }
            match st.vms.console_tail(&st.name, st.cursor) {
                Ok((lines, next)) => {
                    st.cursor = next;
                    if lines.is_empty() {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    } else {
                        st.pending.extend(lines);
                    }
                }
                // VM 被删除 → 结束流
                Err(_) => return None,
            }
        }
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

async fn summarize(
    Json(req): Json<crate::cli::summary::SummarizeApiRequest>,
) -> ApiResult<Json<crate::cli::summary::SummarizeApiResponse>> {
    Ok(Json(crate::cli::summary::summarize_api(req).await?))
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
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": msg })),
        )
            .into_response()
    }
}
