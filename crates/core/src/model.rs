use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Gmail,
    Outlook,
    Imap,
}

impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Provider::Gmail => "gmail",
            Provider::Outlook => "outlook",
            Provider::Imap => "imap",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "gmail" => Some(Provider::Gmail),
            "outlook" => Some(Provider::Outlook),
            "imap" => Some(Provider::Imap),
            _ => None,
        }
    }

    pub fn uses_oauth(self) -> bool {
        !matches!(self, Provider::Imap)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Security {
    /// Implicit TLS from the first byte (IMAPS 993 / SMTPS 465).
    Tls,
    /// Plain connection upgraded with STARTTLS (mandatory, never falls back to plaintext).
    StartTls,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub security: Security,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub provider: Provider,
    pub email: String,
    pub display_name: String,
    /// Username for IMAP/SMTP auth (usually the email address).
    pub username: String,
    pub imap: ServerConfig,
    pub smtp: ServerConfig,
    /// CSS color used to tell accounts apart in the unified UI.
    pub color: String,
    /// Shared mailbox (Microsoft 365): the user who signs in and sends over SMTP, while
    /// `username` (the mailbox address) is used for IMAP. `None` for regular accounts.
    #[serde(default)]
    pub auth_user: Option<String>,
}

impl Account {
    /// User for SMTP authentication: the delegate for shared mailboxes.
    pub fn smtp_user(&self) -> &str {
        self.auth_user.as_deref().unwrap_or(&self.username)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FolderRole {
    Inbox,
    Sent,
    Drafts,
    Junk,
    Trash,
    Archive,
    All,
    Flagged,
    Other,
}

impl FolderRole {
    pub fn as_str(self) -> &'static str {
        match self {
            FolderRole::Inbox => "inbox",
            FolderRole::Sent => "sent",
            FolderRole::Drafts => "drafts",
            FolderRole::Junk => "junk",
            FolderRole::Trash => "trash",
            FolderRole::Archive => "archive",
            FolderRole::All => "all",
            FolderRole::Flagged => "flagged",
            FolderRole::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "inbox" => FolderRole::Inbox,
            "sent" => FolderRole::Sent,
            "drafts" => FolderRole::Drafts,
            "junk" => FolderRole::Junk,
            "trash" => FolderRole::Trash,
            "archive" => FolderRole::Archive,
            "all" => FolderRole::All,
            "flagged" => FolderRole::Flagged,
            _ => FolderRole::Other,
        }
    }

    /// Display order in the sidebar.
    pub fn sort_key(self) -> u8 {
        match self {
            FolderRole::Inbox => 0,
            FolderRole::Flagged => 1,
            FolderRole::Drafts => 2,
            FolderRole::Sent => 3,
            FolderRole::Archive => 4,
            FolderRole::All => 5,
            FolderRole::Junk => 6,
            FolderRole::Trash => 7,
            FolderRole::Other => 8,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: i64,
    pub account_id: String,
    /// Raw IMAP name (modified UTF-7), used for protocol commands.
    pub path: String,
    /// Human readable, decoded name.
    pub name: String,
    pub role: FolderRole,
    pub unread: i64,
    pub total: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmailAddress {
    pub name: Option<String>,
    pub email: String,
}

impl EmailAddress {
    pub fn display(&self) -> String {
        match &self.name {
            Some(n) if !n.is_empty() => format!("{n} <{}>", self.email),
            _ => self.email.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageSummary {
    pub id: i64,
    pub account_id: String,
    pub folder_id: i64,
    pub uid: u32,
    pub message_id: Option<String>,
    pub thread_id: String,
    pub subject: String,
    pub from: Option<EmailAddress>,
    pub to: Vec<EmailAddress>,
    pub cc: Vec<EmailAddress>,
    /// Unix timestamp (seconds).
    pub date: i64,
    pub preview: String,
    pub seen: bool,
    pub flagged: bool,
    pub answered: bool,
    pub has_attachments: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentInfo {
    pub index: usize,
    pub filename: String,
    pub mime_type: String,
    pub size: usize,
    pub inline: bool,
    /// What the UI may do with it (view, open externally, or only save).
    pub kind: crate::attachments::AttachmentKind,
    /// Whether it may be handed to a system application.
    pub can_open: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageBody {
    pub summary: MessageSummary,
    pub reply_to: Vec<EmailAddress>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    /// Sanitized HTML, safe to inject into a sandboxed iframe.
    pub html: Option<String>,
    pub text: Option<String>,
    /// Whether remote content (images, CSS) was stripped from `html`.
    pub remote_content_blocked: bool,
    pub attachments: Vec<AttachmentInfo>,
    /// Reply-only part of the message, without quoted history (for chat view).
    pub chat_text: String,
    /// Same, as strictly sanitized HTML that keeps formatting (lists, tables, bold, links).
    /// Safe to insert into the page directly: no styles, classes or ids.
    pub chat_html: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub thread_id: String,
    pub subject: String,
    /// Senders of the conversation's messages, in order of appearance.
    pub participants: Vec<EmailAddress>,
    /// Recipients of the latest message (shown instead of senders in Sent/Drafts).
    pub recipients: Vec<EmailAddress>,
    pub last_date: i64,
    pub message_count: usize,
    pub unread_count: usize,
    pub flagged: bool,
    pub has_attachments: bool,
    pub preview: String,
    /// Latest message, used to open the thread.
    pub latest_id: i64,
    pub account_id: String,
    /// Pinned by the user: listed above the rest.
    pub pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Signature {
    pub id: String,
    pub account_id: Option<String>,
    pub name: String,
    pub is_html: bool,
    pub body: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutgoingAttachment {
    /// Absolute path on disk, chosen by the user through the file dialog.
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutgoingMessage {
    pub account_id: String,
    pub to: Vec<EmailAddress>,
    #[serde(default)]
    pub cc: Vec<EmailAddress>,
    #[serde(default)]
    pub bcc: Vec<EmailAddress>,
    pub subject: String,
    /// HTML body produced by the rich text editor (signature already included).
    pub html: String,
    /// Plain text alternative.
    pub text: String,
    #[serde(default)]
    pub attachments: Vec<OutgoingAttachment>,
    #[serde(default)]
    pub in_reply_to: Option<String>,
    #[serde(default)]
    pub references: Vec<String>,
    /// When forwarding: row id of the original message whose attachments are included.
    #[serde(default)]
    pub forward_attachments_of: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub close_to_tray: bool,
    pub notifications: bool,
    pub do_not_disturb: bool,
    pub chat_view: bool,
    pub theme: String,
    pub load_remote_images: bool,
    pub start_minimized: bool,
    /// Sidebar reduced to icons only. `serde(default)` keeps settings saved by older
    /// versions loadable (a parse failure would silently reset every setting).
    #[serde(default)]
    pub sidebar_collapsed: bool,
    /// Sound for new mail: one of the bundled sounds ("corneta-suave", "corneta-cartero",
    /// "gaita-breve", played by the UI), "custom" (a file chosen by the user, also played by
    /// the UI), "system" (the notification's own sound) or "none".
    #[serde(default = "default_sound")]
    pub new_mail_sound: String,
    /// Folders hidden from the sidebar, as "{account_id}:{imap path}" (the path survives a
    /// cache rebuild, the folder id doesn't).
    #[serde(default)]
    pub hidden_folders: Vec<String>,
    /// Original file name of the custom new-mail sound (`new_mail_sound == "custom"`); the
    /// file itself is copied into the app's data folder.
    #[serde(default)]
    pub custom_sound_name: Option<String>,
    /// Conversations kept open as tabs under the reading pane, in tab order.
    #[serde(default)]
    pub open_tabs: Vec<OpenTab>,
    /// Key that sends the chat view's quick reply: "enter", "ctrlEnter" or "none".
    #[serde(default = "default_chat_send_key")]
    pub chat_send_key: String,
    /// Key that sends from the full composer: "ctrlEnter" or "none".
    #[serde(default = "default_composer_send_key")]
    pub composer_send_key: String,
    /// Default signatures per account id. Accounts without an entry fall back to the
    /// signatures' own `is_default` flag (how older versions chose it).
    #[serde(default)]
    pub signature_defaults: HashMap<String, SignatureDefaults>,
    /// New-mail sound per account id, overriding `new_mail_sound`.
    #[serde(default)]
    pub account_sounds: HashMap<String, String>,
}

/// Signature ids used by default; `None` means no signature.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SignatureDefaults {
    #[serde(default)]
    pub new: Option<String>,
    #[serde(default)]
    pub reply: Option<String>,
}

impl Settings {
    /// Sound to play for new mail in this account.
    pub fn sound_for(&self, account_id: &str) -> &str {
        self.account_sounds.get(account_id).unwrap_or(&self.new_mail_sound)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpenTab {
    pub account_id: String,
    pub thread_id: String,
    pub subject: String,
}

fn default_sound() -> String {
    "corneta-suave".into()
}

fn default_chat_send_key() -> String {
    "enter".into()
}

fn default_composer_send_key() -> String {
    "ctrlEnter".into()
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            close_to_tray: true,
            notifications: true,
            do_not_disturb: false,
            chat_view: false,
            theme: "system".into(),
            load_remote_images: false,
            start_minimized: false,
            sidebar_collapsed: false,
            new_mail_sound: default_sound(),
            hidden_folders: Vec::new(),
            custom_sound_name: None,
            open_tabs: Vec::new(),
            chat_send_key: default_chat_send_key(),
            composer_send_key: default_composer_send_key(),
            signature_defaults: HashMap::new(),
            account_sounds: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewMailEvent {
    pub account_id: String,
    pub folder_id: i64,
    pub messages: Vec<MessageSummary>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_per_account_overrides_general() {
        let mut s = Settings::default();
        s.new_mail_sound = "gaita-breve".into();
        s.account_sounds.insert("trabajo".into(), "none".into());
        assert_eq!(s.sound_for("trabajo"), "none");
        assert_eq!(s.sound_for("personal"), "gaita-breve");
    }
}
