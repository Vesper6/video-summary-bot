//! 原生桌面窗口（tao + wry WebView2），非浏览器网站。

use std::thread;
use std::time::Duration;

use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::WindowBuilder;

use crate::api;
use crate::error::{Error, Result};
use super::DesktopOpts;

/// 启动桌面应用：后台本地 API + 原生窗口内嵌界面。
pub fn run(opts: DesktopOpts) -> Result<i32> {
    let (port, _server) = api::spawn_local_server()?;
    wait_for_server(port)?;

    let url = format!("http://127.0.0.1:{port}");
    tracing::info!("desktop GUI loading {url}");

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Video Summary Bot — VM Manager")
        .with_inner_size(LogicalSize::new(opts.width, opts.height))
        .with_min_inner_size(LogicalSize::new(960, 600))
        .build(&event_loop)
        .map_err(|e| Error::Cli(format!("create window: {e}")))?;

    let _webview = wry::WebViewBuilder::new()
        .with_url(&url)
        .with_devtools(cfg!(debug_assertions))
        .build(&window)
        .map_err(|e| Error::Cli(format!("create webview: {e}")))?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    });

    Ok(0)
}

fn wait_for_server(port: u16) -> Result<()> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
        .map_err(|e| Error::Cli(format!("http client: {e}")))?;

    for _ in 0..40 {
        if client
            .get(format!("http://127.0.0.1:{port}/api/system/info"))
            .send()
            .is_ok()
        {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(Error::Cli("local API server failed to start".into()))
}