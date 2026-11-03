//! What may be done with an attachment. Allowlist-based: anything not known to be safe can
//! only be saved, never opened from the app (one-click opening is the classic malware path).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AttachmentKind {
    /// Shown in the built-in viewer.
    Image,
    /// Shown in the built-in PDF viewer.
    Pdf,
    /// Opened with the system's application.
    External,
    /// Can only be saved.
    SaveOnly,
}

const IMAGE: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg"];
const EXTERNAL: &[&str] = &[
    "txt", "csv", "md", "rtf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp",
    "zip", "7z", "rar", "tar", "gz", "tgz", "ics", "vcf", "eml", "tif", "tiff", "heic", "heif",
];
/// MIME types that are dangerous whatever the extension claims.
const RISKY_MIME: &[&str] = &[
    "application/x-msdownload", "application/x-msdos-program", "application/x-executable",
    "application/x-sharedlib", "application/x-sh", "application/x-shellscript", "text/x-shellscript",
    "application/javascript", "text/javascript", "application/x-javascript", "text/html",
    "application/xhtml+xml", "application/java-archive", "application/x-ms-shortcut",
    "application/x-desktop", "application/vnd.microsoft.portable-executable", "application/hta",
    "application/vnd.ms-word.document.macroenabled.12", "application/vnd.ms-excel.sheet.macroenabled.12",
    "application/vnd.ms-powerpoint.presentation.macroenabled.12",
];

fn extension(name: &str) -> Option<String> {
    let (stem, ext) = name.trim().rsplit_once('.')?;
    (!stem.is_empty() && !ext.is_empty()).then(|| ext.to_ascii_lowercase())
}

/// Classifies by the file name's extension (what the OS uses to pick an app) and the MIME
/// type: if either is risky or unknown, the attachment can only be saved.
pub fn classify(filename: &str, mime: &str) -> AttachmentKind {
    let mime = mime.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    if RISKY_MIME.contains(&mime.as_str()) {
        return AttachmentKind::SaveOnly;
    }
    match extension(filename).as_deref() {
        Some(e) if IMAGE.contains(&e) => AttachmentKind::Image,
        Some("pdf") => AttachmentKind::Pdf,
        Some(e) if EXTERNAL.contains(&e) => AttachmentKind::External,
        _ => AttachmentKind::SaveOnly,
    }
}

/// Whether the attachment may be handed to a system application. SVG is excluded: outside
/// an `<img>` it opens in a browser, where its scripts would run.
pub fn can_open_externally(filename: &str, mime: &str) -> bool {
    classify(filename, mime) != AttachmentKind::SaveOnly && extension(filename).as_deref() != Some("svg")
}

#[cfg(test)]
mod tests {
    use super::*;
    use AttachmentKind::*;

    #[test]
    fn classification() {
        assert_eq!(classify("Foto.JPG", "image/jpeg"), Image);
        assert_eq!(classify("PI 2026-118.pdf", "application/pdf"), Pdf);
        assert_eq!(classify("informe.docx", "application/octet-stream"), External);
        assert_eq!(classify("factura.pdf.exe", "application/pdf"), SaveOnly);
        assert_eq!(classify("instalar.sh", "text/plain"), SaveOnly);
        assert_eq!(classify("README", "text/plain"), SaveOnly);
        assert_eq!(classify(".pdf", "application/pdf"), SaveOnly);
        assert_eq!(classify("planilla.xlsm", "application/octet-stream"), SaveOnly);
        assert_eq!(classify("datos.csv", "text/html"), SaveOnly);
        assert_eq!(classify("doc.docx", "application/vnd.ms-word.document.macroEnabled.12"), SaveOnly);
        assert_eq!(classify("pagina.html", "text/html; charset=utf-8"), SaveOnly);
    }

    #[test]
    fn external_rules() {
        assert!(can_open_externally("a.pdf", "application/pdf"));
        assert!(can_open_externally("a.png", "image/png"));
        assert!(!can_open_externally("logo.svg", "image/svg+xml"));
        assert!(!can_open_externally("x.exe", "application/octet-stream"));
    }
}
