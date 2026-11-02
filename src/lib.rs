//! vyu — lightweight viewer for Tapo/RTSP cameras on Windows.
//!
//! Module map (see `docs/architecture.md`):
//! - [`app`]: egui UI (canvas, floating bar, dialogs)
//! - [`canvas`]: pure tile geometry
//! - [`stream`]: per-camera sessions and FFmpeg decoding
//! - [`config`], [`secrets`]: persistence and passwords

pub mod app;
pub mod canvas;
pub mod config;
pub mod secrets;
pub mod stream;
