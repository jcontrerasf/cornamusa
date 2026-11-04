mod commands;
mod open_with;
mod tray;

use cornamusa_core::model::Settings;
use cornamusa_core::service::{Event, EventSink, MailService};
use cornamusa_core::store::Store;
use std::sync::{Arc, RwLock};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
#[cfg(not(target_os = "linux"))]
use tauri_plugin_notification::NotificationExt;

pub struct AppState {
    pub service: Arc<MailService>,
    /// Copy of the settings for synchronous checks (close button, notifications).
    pub settings: Arc<RwLock<Settings>>,
}

struct TauriSink {
    app: AppHandle,
    settings: Arc<RwLock<Settings>>,
}

impl EventSink for TauriSink {
    fn emit(&self, event: Event) {
        match &event {
            Event::NewMail(ev) => {
                let s = self.settings.read().unwrap().clone();
                if s.notifications && !s.do_not_disturb {
                    // Bundled sounds are played by the UI; only "system" uses the notification's.
                    notify_new_mail(&self.app, ev, s.new_mail_sound == "system");
                }
            }
            Event::UnreadCount { count } => tray::set_unread(&self.app, *count),
            _ => {}
        }
        let _ = self.app.emit("mail://event", &event);
    }
}

/// Name of the platform's standard new-mail sound, as each notification backend expects it.
const NEW_MAIL_SOUND: &str = if cfg!(target_os = "linux") {
    "message-new-email" // freedesktop sound theme
} else if cfg!(target_os = "windows") {
    "Mail" // ms-winsoundevent:Notification.Mail
} else {
    "default"
};

fn notify_new_mail(app: &AppHandle, ev: &cornamusa_core::model::NewMailEvent, sound: bool) {
    let (title, body) = match ev.messages.as_slice() {
        [] => return,
        [m] => {
            let from = m.from.as_ref().map(|f| f.name.clone().unwrap_or_else(|| f.email.clone())).unwrap_or_default();
            let subject = if m.subject.is_empty() { "(sin asunto)".to_string() } else { m.subject.clone() };
            let body = if m.preview.is_empty() { subject } else { format!("{subject}\n{}", m.preview) };
            (from, body)
        }
        many => (
            format!("{} correos nuevos", many.len()),
            many.iter().take(3).map(|m| format!("• {}", m.subject)).collect::<Vec<_>>().join("\n"),
        ),
    };
    show_notification(app, &title, &body, sound);
}

/// Linux: some notification servers (Cinnamon) play their own sound for every notification
/// unless it carries "suppress-sound", which the notification plugin can't send.
#[cfg(target_os = "linux")]
fn show_notification(app: &AppHandle, title: &str, body: &str, sound: bool) {
    let mut n = notify_rust::Notification::new();
    // "default" is the action the server runs when the notification itself is clicked.
    n.summary(title).body(body).auto_icon().action("default", "Abrir");
    if sound {
        n.sound_name(NEW_MAIL_SOUND);
    } else {
        n.hint(notify_rust::Hint::SuppressSound(true));
    }
    let app = app.clone();
    // notify-rust talks to D-Bus through zbus's blocking API, which panics when called from a
    // tokio worker (mail events come from one), so send it from a thread of its own.
    let _ = std::thread::Builder::new().name("notification".into()).spawn(move || match n.show() {
        // The handle owns the D-Bus connection: keep it until the notification closes, because
        // Cinnamon destroys a windowed app's notifications as soon as their sender leaves the bus.
        Ok(handle) => handle.wait_for_action(|action| {
            if action == "default" {
                show_main_window(&app);
            }
        }),
        Err(e) => tracing::warn!("no se pudo mostrar la notificación: {e}"),
    });
}

#[cfg(not(target_os = "linux"))]
fn show_notification(app: &AppHandle, title: &str, body: &str, sound: bool) {
    let mut builder = app.notification().builder().title(title).body(body);
    if sound {
        builder = builder.sound(NEW_MAIL_SOUND);
    }
    let _ = builder.show();
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Temporary copies of attachments opened with system applications.
pub(crate) fn attachments_dir(app: &AppHandle) -> cornamusa_core::Result<std::path::PathBuf> {
    app.path()
        .app_cache_dir()
        .map(|d| d.join("adjuntos"))
        .map_err(|e| cornamusa_core::Error::Other(e.to_string()))
}

/// Deletes temporary attachment copies older than a day.
fn clean_attachments_dir(app: &AppHandle) {
    let Ok(dir) = attachments_dir(app) else { return };
    let Ok(entries) = std::fs::read_dir(&dir) else { return };
    let day = std::time::Duration::from_secs(24 * 3600);
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > day);
        if old {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

pub fn quit(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        state.service.stop_all();
    }
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "cornamusa=info,cornamusa_core=info".into()),
        )
        .init();

    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        // Launching the app again focuses the running instance instead of opening another.
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main_window(app)));
    }

    builder
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let store = tauri::async_runtime::block_on(Store::open(&data_dir.join("cornamusa.db")))?;
            let settings = tauri::async_runtime::block_on(store.settings())?;
            let settings = Arc::new(RwLock::new(settings));

            let sink = Arc::new(TauriSink { app: app.handle().clone(), settings: settings.clone() });
            let service = MailService::new(store, sink);
            app.manage(AppState { service: service.clone(), settings: settings.clone() });

            tray::create(app.handle())?;
            clean_attachments_dir(app.handle());

            let start_hidden =
                settings.read().unwrap().start_minimized || std::env::args().any(|a| a == "--minimized");
            if !start_hidden {
                show_main_window(app.handle());
            }

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Ok(count) = service.store().unread_inbox_count().await {
                    tray::set_unread(&handle, count);
                }
                if let Err(e) = service.start_all().await {
                    tracing::error!("no se pudieron iniciar las cuentas: {e}");
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let close_to_tray = window
                    .app_handle()
                    .try_state::<AppState>()
                    .map(|s| s.settings.read().unwrap().close_to_tray)
                    .unwrap_or(false);
                if close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    quit(window.app_handle());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_accounts,
            commands::detect_config,
            commands::add_oauth_account,
            commands::add_shared_mailbox,
            commands::add_password_account,
            commands::remove_account,
            commands::update_account,
            commands::list_folders,
            commands::sync_account,
            commands::sync_folder,
            commands::load_older,
            commands::list_threads,
            commands::list_messages,
            commands::get_message,
            commands::get_thread,
            commands::thread_rows,
            commands::set_seen,
            commands::set_flagged,
            commands::move_messages,
            commands::send_message,
            commands::save_draft,
            commands::delete_draft,
            commands::pick_attachments,
            commands::inspect_files,
            commands::save_attachment,
            commands::read_attachment,
            commands::open_attachment,
            commands::open_attachment_with,
            commands::search,
            commands::contacts,
            commands::list_signatures,
            commands::save_signature,
            commands::delete_signature,
            commands::get_settings,
            commands::save_settings,
            commands::trust_sender,
            commands::open_external,
            commands::unread_count,
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar Cornamusa");
}
