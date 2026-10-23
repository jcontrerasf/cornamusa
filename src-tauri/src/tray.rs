//! System tray icon: closing the window keeps Cornamusa running here.

use crate::{quit, show_main_window, AppState};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

const TRAY_ID: &str = "main";

/// Handle to the tray's "No molestar" item, to reflect changes made in the settings screen.
pub struct DndItem(pub CheckMenuItem<tauri::Wry>);

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let dnd_on = app.state::<AppState>().settings.read().unwrap().do_not_disturb;
    let open = MenuItem::with_id(app, "open", "Abrir Cornamusa", true, None::<&str>)?;
    let compose = MenuItem::with_id(app, "compose", "Redactar correo", true, None::<&str>)?;
    let sync = MenuItem::with_id(app, "sync", "Buscar correo nuevo", true, None::<&str>)?;
    let dnd = CheckMenuItem::with_id(app, "dnd", "No molestar", true, dnd_on, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&open, &compose, &sync, &PredefinedMenuItem::separator(app)?, &dnd, &PredefinedMenuItem::separator(app)?, &quit_item],
    )?;

    app.manage(DndItem(dnd.clone()));
    let dnd_item = dnd.clone();
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(app.default_window_icon().cloned().expect("icono de la app"))
        .tooltip("Cornamusa")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "open" => show_main_window(app),
            "compose" => {
                show_main_window(app);
                let _ = app.emit("app://compose", ());
            }
            "sync" => {
                let service = app.state::<AppState>().service.clone();
                tauri::async_runtime::spawn(async move {
                    if let Ok(accounts) = service.store().accounts().await {
                        for a in accounts {
                            let _ = service.sync_account(&a.id).await;
                        }
                    }
                });
            }
            "dnd" => {
                let state = app.state::<AppState>();
                let checked = dnd_item.is_checked().unwrap_or(false);
                let settings = {
                    let mut s = state.settings.write().unwrap();
                    s.do_not_disturb = checked;
                    s.clone()
                };
                let service = state.service.clone();
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = service.store().save_settings(&settings).await;
                    let _ = app.emit("app://settings", &settings);
                });
            }
            "quit" => quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                let app = tray.app_handle();
                match app.get_webview_window("main") {
                    Some(w) if w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false) => {
                        let _ = w.hide();
                    }
                    _ => show_main_window(app),
                }
            }
        })
        .build(app)?;
    Ok(())
}

/// Reflects the unread count in the tray tooltip, the macOS menu bar title and the dock /
/// taskbar badge where the platform supports it.
pub fn set_unread(app: &AppHandle, count: i64) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let tip = if count > 0 { format!("Cornamusa — {count} sin leer") } else { "Cornamusa".to_string() };
        let _ = tray.set_tooltip(Some(tip));
        #[cfg(target_os = "macos")]
        let _ = tray.set_title(if count > 0 { Some(count.to_string()) } else { None });
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_badge_count(if count > 0 { Some(count) } else { None });
    }
}
