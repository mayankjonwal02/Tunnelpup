//! The Cloudflare API token lives only in the OS keychain
//! (macOS Keychain, Windows Credential Manager, Secret Service on Linux).
//! It is never written to the config file or logged.

use crate::error::{AppError, AppResult};

const SERVICE: &str = "com.porthop.app";
const ACCOUNT: &str = "cloudflare-api-token";

fn entry() -> AppResult<keyring::Entry> {
    Ok(keyring::Entry::new(SERVICE, ACCOUNT)?)
}

pub fn save_token(token: &str) -> AppResult<()> {
    entry()?.set_password(token)?;
    Ok(())
}

pub fn load_token() -> AppResult<Option<String>> {
    match entry()?.get_password() {
        Ok(t) => Ok(Some(t)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn require_token() -> AppResult<String> {
    load_token()?.ok_or(AppError::NoToken)
}

pub fn delete_token() -> AppResult<()> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}
