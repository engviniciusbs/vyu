//! Camera passwords live in the Windows Credential Manager, never in config.json.
//! Each camera is stored under service `vyu`, user `<camera_id>`.

use keyring::{Entry, Error};
use uuid::Uuid;

const SERVICE: &str = "vyu";

#[derive(Debug)]
pub struct SecretError(pub String);

impl std::fmt::Display for SecretError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "erro no Gerenciador de Credenciais: {}", self.0)
    }
}

fn entry(id: Uuid) -> Result<Entry, SecretError> {
    Entry::new(SERVICE, &id.to_string()).map_err(|e| SecretError(e.to_string()))
}

pub fn set_password(id: Uuid, pw: &str) -> Result<(), SecretError> {
    entry(id)?.set_password(pw).map_err(|e| SecretError(e.to_string()))
}

pub fn get_password(id: Uuid) -> Result<Option<String>, SecretError> {
    match entry(id)?.get_password() {
        Ok(pw) => Ok(Some(pw)),
        Err(Error::NoEntry) => Ok(None),
        Err(e) => Err(SecretError(e.to_string())),
    }
}

pub fn delete_password(id: Uuid) -> Result<(), SecretError> {
    match entry(id)?.delete_credential() {
        Ok(()) | Err(Error::NoEntry) => Ok(()),
        Err(e) => Err(SecretError(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Touches the real Credential Manager; run with `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn credential_roundtrip() {
        let id = Uuid::new_v4();
        assert_eq!(get_password(id).unwrap(), None);
        set_password(id, "s3nh@").unwrap();
        assert_eq!(get_password(id).unwrap().as_deref(), Some("s3nh@"));
        delete_password(id).unwrap();
        assert_eq!(get_password(id).unwrap(), None);
    }
}
