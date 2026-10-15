use serde::{Serialize, Serializer};

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Cloudflare: {0}")]
    Cloudflare(String),
    #[error("Network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Keychain error: {0}")]
    Keychain(#[from] keyring::Error),
    #[error("No Cloudflare API token saved yet")]
    NoToken,
    #[error("No tunnel has been created for this machine yet")]
    NoTunnel,
    #[error("{0}")]
    Invalid(String),
    #[error("Could not start cloudflared: {0}")]
    Connector(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Tauri(#[from] tauri::Error),
}

// Commands return errors to the UI as plain strings.
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
