use std::sync::{Arc, Mutex};

/// A decoded RGBA video frame. `seq` is assigned by the session and grows by
/// one per published frame, so the UI can tell when to re-upload the texture.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub seq: u64,
}

/// Latest frame only: older frames are overwritten, never queued.
pub type SharedFrame = Arc<Mutex<Option<Frame>>>;
