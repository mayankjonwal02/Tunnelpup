use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Manager, Wry};

use crate::connector::{Connector, ConnectorStatus};

const TRAY_ID: &str = "main";

struct StatusItem(MenuItem<Wry>);

pub fn create(app: &App) -> tauri::Result<()> {
    let status = MenuItem::with_id(app, "status", "Tunnel stopped", false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open PortHop", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit PortHop", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&status, &PredefinedMenuItem::separator(app)?, &open, &quit])?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(app.default_window_icon().cloned().expect("app icon missing"))
        .tooltip("PortHop — Tunnel stopped")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "quit" => {
                app.state::<Connector>().stop(app);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;

    app.manage(StatusItem(status));
    Ok(())
}

pub fn set_status(app: &AppHandle, status: &ConnectorStatus) {
    let label = status.label();
    if let Some(item) = app.try_state::<StatusItem>() {
        let _ = item.0.set_text(&label);
    }
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(format!("PortHop — {label}")));
    }
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}
