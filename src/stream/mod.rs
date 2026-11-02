//! Camera streams: RTSP URLs, session lifecycle and FFmpeg decoding.

pub mod backoff;
pub mod ffmpeg;
pub mod frame;
pub mod quality;
pub mod session;
pub mod url;

/// Tapo exposes two RTSP paths: `stream1` (high quality) and `stream2` (low).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamVariant {
    High,
    Low,
}

impl StreamVariant {
    pub fn path(self) -> &'static str {
        match self {
            StreamVariant::High => "stream1",
            StreamVariant::Low => "stream2",
        }
    }
}
