//! "Iniciar com o sistema": an XDG autostart entry in `~/.config/autostart/vyu.desktop`,
//! honored by GNOME, KDE, XFCE and most other desktops.

use std::io;
use std::path::{Path, PathBuf};

const FILE: &str = "vyu.desktop";

fn autostart_dir() -> io::Result<PathBuf> {
    directories::BaseDirs::new()
        .map(|d| d.config_dir().join("autostart"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "pasta de configuração do usuário não encontrada"))
}

pub fn set_enabled(on: bool) -> io::Result<()> {
    let exe = std::env::current_exe()?;
    set_enabled_at(&autostart_dir()?, &exe, on)
}

pub fn is_enabled() -> bool {
    autostart_dir().map(|d| is_enabled_at(&d)).unwrap_or(false)
}

/// Quotes a path for the desktop entry `Exec` key.
fn exec_quote(path: &Path) -> String {
    let mut out = String::from("\"");
    for c in path.to_string_lossy().chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

fn desktop_entry(exe: &Path) -> String {
    // Desktop-entry strings escape backslashes once more on top of the Exec quoting.
    let exec = exec_quote(exe).replace('\\', "\\\\");
    format!(
        "[Desktop Entry]\nType=Application\nName=vyu\nComment=Visualizador de câmeras Tapo/RTSP\nExec={exec}\nIcon=vyu\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
    )
}

fn set_enabled_at(dir: &Path, exe: &Path, on: bool) -> io::Result<()> {
    let file = dir.join(FILE);
    if on {
        std::fs::create_dir_all(dir)?;
        std::fs::write(file, desktop_entry(exe))
    } else {
        match std::fs::remove_file(file) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

fn is_enabled_at(dir: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(dir.join(FILE)) else { return false };
    // The user may have disabled it in their desktop's startup settings.
    !text.lines().any(|l| matches!(l.trim(), "Hidden=true" | "X-GNOME-Autostart-enabled=false"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enable_then_disable_in_temp_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("autostart");
        let exe = Path::new("/opt/my apps/vyu");
        set_enabled_at(&dir, exe, true).unwrap();
        assert!(is_enabled_at(&dir));
        let text = std::fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(text.contains("Exec=\"/opt/my apps/vyu\"\n"), "{text}");
        set_enabled_at(&dir, exe, false).unwrap();
        assert!(!is_enabled_at(&dir));
        // Disabling twice is fine.
        set_enabled_at(&dir, exe, false).unwrap();
    }

    #[test]
    fn hidden_entry_counts_as_disabled() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(FILE), "[Desktop Entry]\nHidden=true\n").unwrap();
        assert!(!is_enabled_at(tmp.path()));
    }

    #[test]
    fn exec_quote_escapes_specials() {
        assert_eq!(exec_quote(Path::new(r#"/a "b"/$c"#)), r#""/a \"b\"/\$c""#);
    }
}
