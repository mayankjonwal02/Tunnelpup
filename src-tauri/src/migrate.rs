//! TunnelPup was called PortHop until October 2026. On a computer that ran PortHop, TunnelPup
//! takes over its config file (account, tunnel, routes) and API token the first time it starts,
//! so the routes keep working.

use std::path::Path;

use tauri::{AppHandle, Manager};

use crate::{config, secrets};

/// PortHop's app identifier, which its config folder and keychain entry were named after.
const PORTHOP_ID: &str = "com.porthop.app";

pub fn from_porthop(app: &AppHandle) {
    if let Ok(dir) = app.path().app_config_dir()
        && let Some(parent) = dir.parent()
        && let Err(e) = move_config(&parent.join(PORTHOP_ID), &dir)
    {
        eprintln!("[tunnelpup] could not take over PortHop's settings: {e}");
    }
    if let Err(e) = secrets::adopt_token(PORTHOP_ID) {
        eprintln!("[tunnelpup] could not take over PortHop's token: {e}");
    }
}

/// What PortHop called the tunnel that TunnelPup calls `name`: `porthop-<computer>`.
pub fn porthop_tunnel_name(name: &str) -> String {
    name.replacen("tunnelpup-", "porthop-", 1)
}

/// Move PortHop's config file into TunnelPup's folder, unless TunnelPup already has one.
fn move_config(old_dir: &Path, new_dir: &Path) -> std::io::Result<()> {
    let (old, new) = (old_dir.join(config::FILE), new_dir.join(config::FILE));
    if new.exists() || !old.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(new_dir)?;
    std::fs::rename(&old, &new).or_else(|_| std::fs::copy(&old, &new).map(drop))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_porthops_config_only_when_tunnelpup_has_none() {
        let root = std::env::temp_dir().join(format!("tunnelpup-migrate-{}", std::process::id()));
        let (old_dir, new_dir) = (root.join("com.porthop.app"), root.join("io.github.mayankjonwal02.tunnelpup"));
        std::fs::create_dir_all(&old_dir).unwrap();
        let read = |dir: &Path| std::fs::read_to_string(dir.join(config::FILE)).ok();

        std::fs::write(old_dir.join(config::FILE), "porthop").unwrap();
        move_config(&old_dir, &new_dir).unwrap();
        assert_eq!(read(&new_dir).as_deref(), Some("porthop"));
        assert_eq!(read(&old_dir), None);

        std::fs::write(old_dir.join(config::FILE), "stale").unwrap();
        move_config(&old_dir, &new_dir).unwrap();
        assert_eq!(read(&new_dir).as_deref(), Some("porthop"));

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn knows_porthops_tunnel_name() {
        assert_eq!(porthop_tunnel_name("tunnelpup-alex-laptop"), "porthop-alex-laptop");
    }
}
