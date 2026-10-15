//! The Cloudflare API token lives only in the OS keychain
//! (macOS Keychain, Windows Credential Manager, Secret Service on Linux).
//! It is never written to the config file or logged.

use crate::error::{AppError, AppResult};

const SERVICE: &str = "io.github.mayankjonwal02.tunnelpup";
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

/// Take over a token saved under another app identifier, unless TunnelPup already has one.
/// The old entry is removed either way.
pub fn adopt_token(old_service: &str) -> AppResult<()> {
    move_token(&keyring::Entry::new(old_service, ACCOUNT)?, &entry()?)
}

fn move_token(from: &keyring::Entry, to: &keyring::Entry) -> AppResult<()> {
    let token = match from.get_password() {
        Ok(t) => t,
        Err(keyring::Error::NoEntry) => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    match to.get_password() {
        Ok(_) => {}
        Err(keyring::Error::NoEntry) => to.set_password(&token)?,
        Err(e) => return Err(e.into()),
    }
    match from.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn moves_a_token_but_keeps_one_already_there() {
        let from = keyring::Entry::new("io.github.mayankjonwal02.tunnelpup.test-old", ACCOUNT).unwrap();
        let to = keyring::Entry::new("io.github.mayankjonwal02.tunnelpup.test-new", ACCOUNT).unwrap();
        let _ = (from.delete_credential(), to.delete_credential());

        from.set_password("first").unwrap();
        move_token(&from, &to).unwrap();
        assert_eq!(to.get_password().unwrap(), "first");
        assert!(matches!(from.get_password(), Err(keyring::Error::NoEntry)));

        from.set_password("stale").unwrap();
        move_token(&from, &to).unwrap();
        assert_eq!(to.get_password().unwrap(), "first");
        assert!(matches!(from.get_password(), Err(keyring::Error::NoEntry)));

        to.delete_credential().unwrap();
    }
}
