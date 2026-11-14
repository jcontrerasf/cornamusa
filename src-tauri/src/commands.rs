//! Commands exposed to the frontend. They are thin wrappers over `MailService`.

use crate::AppState;
use cornamusa_core::attachments::{can_open_externally, classify, AttachmentKind};
use cornamusa_core::mcp::DraftState;
use cornamusa_core::mime::sanitize_filename;
use cornamusa_core::model::*;
use cornamusa_core::providers::DetectedConfig;
use cornamusa_core::service::{MoveTarget, PasswordAccountRequest};
use cornamusa_core::{Error, Result};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

type Cmd<T> = Result<T>;

// ---------------------------------------------------------------- accounts

#[tauri::command]
pub async fn list_accounts(state: State<'_, AppState>) -> Cmd<Vec<Account>> {
    state.service.store().accounts().await
}

#[tauri::command]
pub async fn detect_config(state: State<'_, AppState>, email: String) -> Cmd<DetectedConfig> {
    Ok(state.service.detect(&email).await)
}

#[tauri::command]
pub async fn add_oauth_account(
    app: AppHandle,
    state: State<'_, AppState>,
    provider: Provider,
    login_hint: Option<String>,
) -> Cmd<Account> {
    state
        .service
        .add_oauth_account(provider, login_hint, |url| {
            app.opener()
                .open_url(url, None::<&str>)
                .map_err(|e| Error::Other(format!("No se pudo abrir el navegador: {e}")))
        })
        .await
}

#[tauri::command]
pub async fn add_shared_mailbox(
    app: AppHandle,
    state: State<'_, AppState>,
    shared_email: String,
    login_hint: Option<String>,
) -> Cmd<Account> {
    state
        .service
        .add_shared_mailbox(shared_email, login_hint, |url| {
            app.opener()
                .open_url(url, None::<&str>)
                .map_err(|e| Error::Other(format!("No se pudo abrir el navegador: {e}")))
        })
        .await
}

#[tauri::command]
pub async fn add_password_account(state: State<'_, AppState>, request: PasswordAccountRequest) -> Cmd<Account> {
    state.service.add_password_account(request).await
}

#[tauri::command]
pub async fn remove_account(state: State<'_, AppState>, id: String) -> Cmd<()> {
    state.service.remove_account(&id).await
}

#[tauri::command]
pub async fn update_account(state: State<'_, AppState>, id: String, display_name: String, color: String) -> Cmd<()> {
    state.service.update_account(&id, &display_name, &color).await
}

// ---------------------------------------------------------------- folders & lists

#[tauri::command]
pub async fn list_folders(state: State<'_, AppState>, account_id: String) -> Cmd<Vec<Folder>> {
    state.service.store().folders(&account_id).await
}

#[tauri::command]
pub async fn sync_account(state: State<'_, AppState>, account_id: String) -> Cmd<()> {
    state.service.sync_account(&account_id).await
}

#[tauri::command]
pub async fn sync_folder(state: State<'_, AppState>, folder_id: i64) -> Cmd<()> {
    state.service.sync_folder_by_id(folder_id).await
}

#[tauri::command]
pub async fn load_older(state: State<'_, AppState>, folder_id: i64) -> Cmd<usize> {
    state.service.load_older(folder_id, 200).await
}

#[tauri::command]
pub async fn list_threads(state: State<'_, AppState>, folder_ids: Vec<i64>, limit: i64, offset: i64) -> Cmd<Vec<Thread>> {
    state.service.store().threads(&folder_ids, limit, offset).await
}

#[tauri::command]
pub async fn list_messages(state: State<'_, AppState>, folder_id: i64, limit: i64, offset: i64) -> Cmd<Vec<MessageSummary>> {
    state.service.store().messages(folder_id, limit, offset).await
}

#[tauri::command]
pub async fn set_thread_pinned(state: State<'_, AppState>, account_id: String, thread_id: String, pinned: bool) -> Cmd<()> {
    state.service.store().set_thread_pinned(&account_id, &thread_id, pinned).await
}

// ---------------------------------------------------------------- reading

#[tauri::command]
pub async fn get_message(state: State<'_, AppState>, id: i64, force_remote: bool) -> Cmd<MessageBody> {
    state.service.message(id, force_remote).await
}

#[tauri::command]
pub async fn get_thread(
    state: State<'_, AppState>,
    account_id: String,
    thread_id: String,
    force_remote: bool,
) -> Cmd<Vec<MessageBody>> {
    state.service.thread(&account_id, &thread_id, force_remote).await
}

#[tauri::command]
pub async fn thread_rows(
    state: State<'_, AppState>,
    account_id: String,
    thread_id: String,
    folder_ids: Vec<i64>,
) -> Cmd<Vec<i64>> {
    state.service.thread_rows(&account_id, &thread_id, &folder_ids).await
}

// ---------------------------------------------------------------- actions

#[tauri::command]
pub async fn set_seen(state: State<'_, AppState>, ids: Vec<i64>, seen: bool) -> Cmd<()> {
    state.service.set_seen(&ids, seen).await
}

#[tauri::command]
pub async fn set_flagged(state: State<'_, AppState>, ids: Vec<i64>, flagged: bool) -> Cmd<()> {
    state.service.set_flagged(&ids, flagged).await
}

#[tauri::command]
pub async fn move_messages(state: State<'_, AppState>, ids: Vec<i64>, target: MoveTarget) -> Cmd<()> {
    state.service.move_messages(&ids, target).await
}

// ---------------------------------------------------------------- composing

#[tauri::command]
pub async fn send_message(state: State<'_, AppState>, message: OutgoingMessage) -> Cmd<String> {
    state.service.send(&message).await
}

#[tauri::command]
pub async fn save_draft(state: State<'_, AppState>, message: OutgoingMessage, replaces: Option<String>) -> Cmd<String> {
    state.service.save_draft(&message, replaces).await
}

#[tauri::command]
pub async fn delete_draft(state: State<'_, AppState>, account_id: String, message_id: String) -> Cmd<()> {
    state.service.delete_draft(&account_id, &message_id).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PickedFile {
    path: String,
    name: String,
    size: u64,
}

fn file_info(path: std::path::PathBuf) -> Option<PickedFile> {
    let meta = std::fs::metadata(&path).ok().filter(|m| m.is_file())?;
    Some(PickedFile {
        name: path.file_name()?.to_string_lossy().into_owned(),
        size: meta.len(),
        path: path.to_string_lossy().into_owned(),
    })
}

/// Native file picker for attachments.
#[tauri::command]
pub async fn pick_attachments(app: AppHandle) -> Cmd<Vec<PickedFile>> {
    let picked = tokio::task::spawn_blocking(move || app.dialog().file().set_title("Adjuntar archivos").blocking_pick_files())
        .await
        .map_err(|e| Error::Other(e.to_string()))?;
    Ok(picked
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| p.into_path().ok())
        .filter_map(file_info)
        .collect())
}

/// Metadata for files dropped onto the composer.
#[tauri::command]
pub async fn inspect_files(paths: Vec<String>) -> Cmd<Vec<PickedFile>> {
    Ok(paths.into_iter().map(std::path::PathBuf::from).filter_map(file_info).collect())
}

/// Lets the user choose where to save an attachment. Attachments are never opened
/// automatically.
#[tauri::command]
pub async fn save_attachment(app: AppHandle, state: State<'_, AppState>, message_id: i64, index: usize) -> Cmd<bool> {
    let name = state.service.attachment_name(message_id, index).await?;
    let dest = tokio::task::spawn_blocking(move || {
        app.dialog().file().set_title("Guardar adjunto").set_file_name(name).blocking_save_file()
    })
    .await
    .map_err(|e| Error::Other(e.to_string()))?;
    let Some(dest) = dest.and_then(|d| d.into_path().ok()) else { return Ok(false) };
    state.service.save_attachment(message_id, index, &dest).await?;
    Ok(true)
}

/// Bytes of an image/PDF attachment for the built-in viewer (raw binary, no base64).
#[tauri::command]
pub async fn read_attachment(state: State<'_, AppState>, message_id: i64, index: usize) -> Cmd<tauri::ipc::Response> {
    let (name, mime, bytes) = state.service.attachment_bytes(message_id, index).await?;
    match classify(&name, &mime) {
        AttachmentKind::Image | AttachmentKind::Pdf => Ok(tauri::ipc::Response::new(bytes)),
        _ => Err(Error::Other("Este tipo de adjunto no se puede previsualizar".into())),
    }
}

// ---------------------------------------------------------------- custom sound

const SOUND_EXTENSIONS: &[&str] = &["wav", "mp3", "ogg", "oga", "opus", "flac", "m4a", "aac", "webm"];
const MAX_SOUND_BYTES: u64 = 5 * 1024 * 1024;

fn sounds_dir(app: &AppHandle) -> Result<std::path::PathBuf> {
    app.path().app_data_dir().map(|d| d.join("sounds")).map_err(|e| Error::Other(e.to_string()))
}

/// The copied custom sound, whatever its extension.
fn custom_sound_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    std::fs::read_dir(sounds_dir(app).ok()?)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.file_stem().is_some_and(|s| s == "custom"))
}

/// Lets the user pick an audio file for new mail and copies it into the app's data folder, so
/// it keeps working if the original is moved. Returns the original file name, `None` if the
/// dialog was cancelled.
#[tauri::command]
pub async fn pick_custom_sound(app: AppHandle) -> Cmd<Option<String>> {
    let dialog_app = app.clone();
    let picked = tokio::task::spawn_blocking(move || {
        dialog_app.dialog().file().set_title("Elegir sonido").add_filter("Audio", SOUND_EXTENSIONS).blocking_pick_file()
    })
    .await
    .map_err(|e| Error::Other(e.to_string()))?;
    let Some(src) = picked.and_then(|p| p.into_path().ok()) else { return Ok(None) };
    let ext = src.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if !SOUND_EXTENSIONS.contains(&ext.as_str()) {
        return Err(Error::Other("Ese archivo no es un formato de audio compatible".into()));
    }
    if tokio::fs::metadata(&src).await?.len() > MAX_SOUND_BYTES {
        return Err(Error::Other("El sonido es demasiado grande (máximo 5 MB)".into()));
    }
    let dir = sounds_dir(&app)?;
    tokio::fs::create_dir_all(&dir).await?;
    if let Some(old) = custom_sound_path(&app) {
        let _ = tokio::fs::remove_file(old).await;
    }
    tokio::fs::copy(&src, dir.join(format!("custom.{ext}"))).await?;
    Ok(src.file_name().map(|n| n.to_string_lossy().into_owned()))
}

/// Bytes of the custom sound, decoded by the UI's Web Audio (raw binary, no base64).
#[tauri::command]
pub async fn read_custom_sound(app: AppHandle) -> Cmd<tauri::ipc::Response> {
    let path = custom_sound_path(&app).ok_or_else(|| Error::Other("No hay un sonido personalizado".into()))?;
    Ok(tauri::ipc::Response::new(tokio::fs::read(path).await?))
}

/// Writes an attachment to the temporary folder and returns its path. Refuses anything that
/// isn't on the allowlist: risky types can only be saved.
async fn temp_copy(app: &AppHandle, state: &AppState, message_id: i64, index: usize) -> Result<std::path::PathBuf> {
    let (name, mime, bytes) = state.service.attachment_bytes(message_id, index).await?;
    if !can_open_externally(&name, &mime) {
        return Err(Error::Other(format!(
            "Por seguridad, «{name}» no se puede abrir desde Cornamusa. Guárdalo y revísalo antes de abrirlo."
        )));
    }
    let dir = crate::attachments_dir(app)?.join(format!("{message_id}-{index}"));
    tokio::fs::create_dir_all(&dir).await?;
    let path = dir.join(sanitize_filename(&name));
    tokio::fs::write(&path, bytes).await?;
    Ok(path)
}

/// Opens an attachment with the system's default application.
#[tauri::command]
pub async fn open_attachment(app: AppHandle, state: State<'_, AppState>, message_id: i64, index: usize) -> Cmd<()> {
    let path = temp_copy(&app, &state, message_id, index).await?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| Error::Other(format!("No se pudo abrir el adjunto: {e}")))
}

/// "Abrir con…": lets the user pick the application. Returns false when it fell back to the
/// default application (no chooser available).
#[tauri::command]
pub async fn open_attachment_with(app: AppHandle, state: State<'_, AppState>, message_id: i64, index: usize) -> Cmd<bool> {
    let path = temp_copy(&app, &state, message_id, index).await?;
    crate::open_with::choose_and_open(&app, &path).await
}

// ---------------------------------------------------------------- search

#[tauri::command]
pub async fn search(state: State<'_, AppState>, query: String, account_id: Option<String>) -> Cmd<Vec<MessageSummary>> {
    state.service.store().search(&query, account_id.as_deref(), 200).await
}

#[tauri::command]
pub async fn contacts(state: State<'_, AppState>, prefix: String) -> Cmd<Vec<EmailAddress>> {
    state.service.store().contacts(&prefix, 8).await
}

// ---------------------------------------------------------------- signatures & settings

#[tauri::command]
pub async fn list_signatures(state: State<'_, AppState>) -> Cmd<Vec<Signature>> {
    state.service.store().signatures().await
}

#[tauri::command]
pub async fn save_signature(state: State<'_, AppState>, signature: Signature) -> Cmd<Signature> {
    state.service.store().save_signature(&signature).await
}

#[tauri::command]
pub async fn delete_signature(state: State<'_, AppState>, id: String) -> Cmd<()> {
    state.service.store().delete_signature(&id).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    #[serde(flatten)]
    settings: Settings,
    autostart: bool,
}

#[tauri::command]
pub async fn get_settings(app: AppHandle, state: State<'_, AppState>) -> Cmd<SettingsView> {
    let settings = state.service.store().settings().await?;
    Ok(SettingsView { settings, autostart: app.autolaunch().is_enabled().unwrap_or(false) })
}

#[tauri::command]
pub async fn save_settings(app: AppHandle, state: State<'_, AppState>, settings: Settings, autostart: bool) -> Cmd<()> {
    state.service.store().save_settings(&settings).await?;
    *state.settings.write().unwrap() = settings.clone();
    crate::apply_mcp(&app, settings.mcp_enabled).await;
    if let Some(dnd) = app.try_state::<crate::tray::DndItem>() {
        let _ = dnd.0.set_checked(settings.do_not_disturb);
    }
    let al = app.autolaunch();
    let result = if autostart { al.enable() } else { al.disable() };
    if let Err(e) = result {
        tracing::warn!("autostart: {e}");
    }
    let _ = app.emit("app://settings", &settings);
    Ok(())
}

/// Answer of the window to a draft change asked by Claude (see `TauriMcpUi::change_draft`).
#[tauri::command]
pub fn mcp_draft_result(state: State<'_, AppState>, request_id: u64, draft: Option<DraftState>, error: Option<String>) {
    if let Some(reply) = state.draft_replies.lock().unwrap().remove(&request_id) {
        let _ = reply.send(draft.ok_or_else(|| error.unwrap_or_else(|| "No se pudo cambiar el borrador".into())));
    }
}

/// Path of this executable, for the MCP setup shown in Settings (`<path> mcp`).
#[tauri::command]
pub fn mcp_command() -> Cmd<String> {
    // An AppImage runs from a temporary mount; the stable path is the .AppImage file itself.
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        return Ok(appimage.to_string_lossy().into_owned());
    }
    Ok(std::env::current_exe()?.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn trust_sender(state: State<'_, AppState>, email: String) -> Cmd<()> {
    state.service.trust_sender(&email).await
}

/// Opens a link from an email in the system browser. Only web links are allowed: never
/// `file:`, custom schemes or anything that could launch a local program.
#[tauri::command]
pub async fn open_external(app: AppHandle, url: String) -> Cmd<()> {
    let parsed = url::Url::parse(&url).map_err(|_| Error::Other("Enlace inválido".into()))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(Error::Other(format!("Tipo de enlace no permitido: {}", parsed.scheme())));
    }
    app.opener()
        .open_url(parsed.as_str(), None::<&str>)
        .map_err(|e| Error::Other(e.to_string()))
}

#[tauri::command]
pub async fn unread_count(state: State<'_, AppState>) -> Cmd<i64> {
    state.service.store().unread_inbox_count().await
}
