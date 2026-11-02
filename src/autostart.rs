//! "Iniciar com o Windows": a value in HKCU\Software\Microsoft\Windows\CurrentVersion\Run.

use std::io;

use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "vyu";

pub fn set_enabled(on: bool) -> io::Result<()> {
    let exe = std::env::current_exe()?;
    set_enabled_at(RUN_KEY, &exe.to_string_lossy(), on)
}

pub fn is_enabled() -> bool {
    is_enabled_at(RUN_KEY)
}

fn set_enabled_at(key: &str, exe: &str, on: bool) -> io::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (run, _) = hkcu.create_subkey_with_flags(key, KEY_READ | KEY_WRITE)?;
    if on {
        run.set_value(VALUE, &format!("\"{exe}\""))
    } else {
        match run.delete_value(VALUE) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

fn read_value(key: &str) -> io::Result<String> {
    RegKey::predef(HKEY_CURRENT_USER).open_subkey(key)?.get_value(VALUE)
}

fn is_enabled_at(key: &str) -> bool {
    read_value(key).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_KEY: &str = r"Software\vyu-test-autostart";

    #[test]
    fn enable_then_disable_in_test_key() {
        let exe = r"C:\Program Files\vyu\vyu.exe";
        set_enabled_at(TEST_KEY, exe, true).unwrap();
        assert!(is_enabled_at(TEST_KEY));
        assert_eq!(read_value(TEST_KEY).unwrap(), format!("\"{exe}\""));
        set_enabled_at(TEST_KEY, exe, false).unwrap();
        assert!(!is_enabled_at(TEST_KEY));
        // Disabling twice is fine.
        set_enabled_at(TEST_KEY, exe, false).unwrap();
        let _ = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).delete_subkey(TEST_KEY);
    }
}
