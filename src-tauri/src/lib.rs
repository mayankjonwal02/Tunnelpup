mod cloudflare;
mod commands;
mod config;
mod connector;
mod doh;
mod error;
mod freedomain;
mod guard;
mod ports;
mod secrets;
mod tray;
mod verify;

use std::sync::Mutex;

use tauri::{Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

use commands::{ConfigState, RouteLock};
use connector::Connector;

/// Keep in sync with scripts/fetch-cloudflared.mjs.
pub const CLOUDFLARED_VERSION: &str = "2026.10.0";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        // Must be registered first: a second launch just focuses the running app.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| tray::show_main(app)))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(Connector::default())
        .manage(RouteLock(tokio::sync::Mutex::new(())))
        .setup(|app| {
            let handle = app.handle().clone();
            // A connector left behind by a TunnelPup that crashed or was killed.
            guard::kill_leftover(&handle);
            let cfg = config::load(&handle);
            app.manage(ConfigState(Mutex::new(cfg.clone())));
            tray::create(app)?;

            // Launched by autostart: stay in the tray.
            if std::env::args().any(|a| a == "--minimized")
                && let Some(w) = app.get_webview_window("main")
            {
                let _ = w.hide();
            }

            // Bring the tunnel back up after a reboot / relaunch.
            if let (Some(account), Some(tunnel)) = (cfg.account_id, cfg.tunnel) {
                tauri::async_runtime::spawn(async move {
                    let Ok(Some(token)) = secrets::load_token() else { return };
                    let Ok(client) = cloudflare::Cloudflare::new(token) else { return };
                    match client.tunnel_token(&account, &tunnel.id).await {
                        Ok(t) => {
                            let _ = handle.state::<Connector>().start(&handle, t);
                        }
                        Err(e) => eprintln!("[tunnelpup] could not auto-start tunnel: {e}"),
                    }
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps TunnelPup running in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::get_connector_logs,
            commands::save_token,
            commands::select_account,
            commands::sign_out,
            commands::list_zones,
            commands::add_zone,
            commands::check_zone,
            commands::check_free_domain,
            commands::check_https,
            commands::ensure_tunnel,
            commands::start_tunnel,
            commands::stop_tunnel,
            commands::add_route,
            commands::update_route_port,
            commands::remove_route,
            commands::suggest_ports,
            commands::check_ports,
        ])
        .build(tauri::generate_context!())
        .expect("error while building TunnelPup");

    app.run(|app, event| match event {
        // Never leave an orphaned cloudflared behind.
        RunEvent::Exit => app.state::<Connector>().stop(app),
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { .. } => tray::show_main(app),
        _ => {}
    });
}
