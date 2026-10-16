//! Non-secret app state, persisted as JSON in the OS app-config directory.
//! Cloudflare is the source of truth for routing; this is a local mirror so the
//! UI can render instantly and we remember which DNS records we created.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::error::AppResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub account_id: Option<String>,
    pub account_name: Option<String>,
    pub tunnel: Option<TunnelInfo>,
    #[serde(default)]
    pub routes: Vec<Route>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    /// Full public hostname, e.g. `app.example.com`.
    pub hostname: String,
    pub zone_id: String,
    pub port: u16,
    /// ID of the CNAME we created, so we can delete exactly that record later.
    pub dns_record_id: Option<String>,
    /// Send the app `Host: localhost:<port>` instead of the public hostname, for apps that
    /// refuse any other Host (Ollama, Vite, Django, Rails…).
    #[serde(default)]
    pub rewrite_host: bool,
}

fn path(app: &AppHandle) -> AppResult<PathBuf> {
    let dir = app.path().app_config_dir()?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("config.json"))
}

pub fn load(app: &AppHandle) -> AppConfig {
    path(app).ok().and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save(app: &AppHandle, cfg: &AppConfig) -> AppResult<()> {
    let p = path(app)?;
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(cfg)?)?;
    std::fs::rename(tmp, p)?; // atomic replace
    Ok(())
}
