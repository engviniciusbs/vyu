use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::Config;

/// `%APPDATA%\vyu`
pub fn config_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|d| d.config_dir().join("vyu"))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

pub struct LoadOutcome {
    pub config: Config,
    /// Set when the file existed but could not be parsed; it was moved here.
    pub recovered_backup: Option<PathBuf>,
}

pub fn load(path: &Path) -> LoadOutcome {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return LoadOutcome { config: Config::default(), recovered_backup: None },
    };
    match serde_json::from_str::<Config>(&text) {
        Ok(config) => LoadOutcome { config, recovered_backup: None },
        Err(err) => {
            let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let backup = path.with_file_name(format!("config.json.bak-{secs}"));
            tracing::warn!("config inválida ({err}); movida para {}", backup.display());
            let recovered = std::fs::rename(path, &backup).ok().map(|_| backup);
            LoadOutcome { config: Config::default(), recovered_backup: recovered }
        }
    }
}

/// Atomic save: write `config.json.tmp`, then rename over the real file.
pub fn save(path: &Path, cfg: &Config) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_file_name("config.json.tmp");
    let json = serde_json::to_string_pretty(cfg).map_err(io::Error::other)?;
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Camera, Config};

    #[test]
    fn roundtrip_preserves_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mut cfg = Config::default();
        cfg.cameras.push(Camera::new("Sala", "192.168.0.20", "cam"));
        cfg.settings.start_fullscreen = true;

        save(&path, &cfg).unwrap();
        let out = load(&path);

        assert_eq!(out.config, cfg);
        assert!(out.recovered_backup.is_none());
    }

    #[test]
    fn missing_file_gives_default() {
        let dir = tempfile::tempdir().unwrap();
        let out = load(&dir.path().join("config.json"));
        assert_eq!(out.config, Config::default());
        assert!(out.recovered_backup.is_none());
    }

    #[test]
    fn corrupt_file_is_backed_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, "{not json").unwrap();

        let out = load(&path);

        assert_eq!(out.config, Config::default());
        let backup = out.recovered_backup.expect("backup path");
        let name = backup.file_name().unwrap().to_string_lossy().to_string();
        assert!(name.starts_with("config.json.bak-"), "{name}");
        assert_eq!(std::fs::read_to_string(backup).unwrap(), "{not json");
    }

    #[test]
    fn save_leaves_no_tmp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        save(&path, &Config::default()).unwrap();
        assert!(!dir.path().join("config.json.tmp").exists());
    }
}
