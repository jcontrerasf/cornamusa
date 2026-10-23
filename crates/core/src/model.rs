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
