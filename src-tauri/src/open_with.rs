//! "Abrir con…": shows the platform's application chooser for a file.

use cornamusa_core::{Error, Result};
use std::path::Path;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

fn open_default(app: &AppHandle, path: &Path) -> Result<()> {
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| Error::Other(format!("No se pudo abrir el adjunto: {e}")))
}

/// Returns `Ok(true)` when the chooser was shown, `Ok(false)` when it fell back to the
/// default application.
#[cfg(target_os = "linux")]
pub async fn choose_and_open(app: &AppHandle, path: &Path) -> Result<bool> {
    use ashpd::desktop::open_uri::OpenFileRequest;
    use std::os::fd::AsFd;

    let file = std::fs::File::open(path)?;
    let request = OpenFileRequest::default().ask(true).send_file(&file.as_fd()).await;
    match request.and_then(|r| r.response()) {
        Ok(()) => Ok(true),
        // The user closed the chooser: nothing to do.
        Err(ashpd::Error::Response(ashpd::desktop::ResponseError::Cancelled)) => Ok(true),
        Err(e) => {
            tracing::warn!("portal OpenURI no disponible: {e}");
            open_default(app, path)?;
            Ok(false)
        }
    }
}

#[cfg(target_os = "windows")]
pub async fn choose_and_open(_app: &AppHandle, path: &Path) -> Result<bool> {
    std::process::Command::new("rundll32")
        .arg("shell32.dll,OpenAs_RunDLL")
        .arg(path)
        .spawn()
        .map_err(|e| Error::Other(format!("No se pudo mostrar «Abrir con»: {e}")))?;
    Ok(true)
}

#[cfg(target_os = "macos")]
pub async fn choose_and_open(app: &AppHandle, path: &Path) -> Result<bool> {
    use tauri_plugin_dialog::DialogExt;
    let dialog = app.dialog().clone();
    let picked = tokio::task::spawn_blocking(move || {
        dialog.file().set_title("Elegir aplicación").set_directory("/Applications").blocking_pick_file()
    })
    .await
    .map_err(|e| Error::Other(e.to_string()))?;
    let Some(application) = picked.and_then(|p| p.into_path().ok()) else { return Ok(true) };
    std::process::Command::new("open")
        .arg("-a")
        .arg(application)
        .arg(path)
        .spawn()
        .map_err(|e| Error::Other(format!("No se pudo abrir el adjunto: {e}")))?;
    Ok(true)
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub async fn choose_and_open(app: &AppHandle, path: &Path) -> Result<bool> {
    open_default(app, path)?;
    Ok(false)
}
