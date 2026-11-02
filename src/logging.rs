//! Logs to `%APPDATA%\vyu\logs\vyu.log.<date>` (daily rotation).
//! Level comes from `RUST_LOG` (default `info`). Never log raw RTSP URLs:
//! use `stream::url::mask`.

use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

/// Keep the returned guard alive for the whole program, or logs are lost.
pub fn init(dir: &Path) -> Option<WorkerGuard> {
    std::fs::create_dir_all(dir).ok()?;
    let appender = tracing_appender::rolling::daily(dir, "vyu.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,wgpu_hal=error,wgpu_core=warn,naga=warn,egui_wgpu=warn"));
    tracing_subscriber::fmt().with_env_filter(filter).with_writer(writer).with_ansi(false).try_init().ok()?;
    Some(guard)
}
