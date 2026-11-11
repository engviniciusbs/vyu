//! Start vyu when the user logs in. Windows: registry `Run` key.
//! Linux: an XDG autostart entry (`~/.config/autostart/vyu.desktop`).

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{is_enabled, set_enabled};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{is_enabled, set_enabled};

/// Label of the checkbox in Settings.
#[cfg(windows)]
pub const LABEL: &str = "Iniciar com o Windows";
#[cfg(not(windows))]
pub const LABEL: &str = "Iniciar com o sistema";
