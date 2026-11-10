//! Local cache in SQLite. Secrets are never stored here (see `secrets`).

use crate::error::{Error, Result};
use crate::mime::ParsedHeaders;
use crate::model::*;
use crate::sanitize::sanitize_signature_html;
use crate::threading::{ancestors, thread_root};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions, SqliteRow};
use sqlx::Row;
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;

#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Flags {
    pub seen: bool,
    pub flagged: bool,
    pub answered: bool,
}

/// Server-side facts about a message, from the header fetch.
#[derive(Debug, Clone, Copy)]
pub struct HeaderMeta {
    pub uid: u32,
    pub flags: Flags,
    pub internal_date: Option<i64>,
    pub size: u32,
}

pub struct FolderSync {
    pub uid_validity: Option<u32>,
    /// Highest UID seen on the server; `None` means the folder was never synced.
    pub last_uid: Option<u32>,
    pub max_uid: Option<u32>,
    pub min_uid: Option<u32>,
}

/// Where a message lives on the server.
#[derive(Debug, Clone)]
pub struct MessageLocation {
    pub id: i64,
    pub account_id: String,
    pub folder_id: i64,
    pub folder_path: String,
    pub folder_role: FolderRole,
    pub uid: u32,
}

fn json<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "null".into())
}

fn from_json<T: serde::de::DeserializeOwned + Default>(s: Option<String>) -> T {
    s.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

const SUMMARY_COLS: &str = "id, account_id, folder_id, uid, message_id, thread_id, subject, from_json, \
    to_json, cc_json, date, preview, seen, flagged, answered, has_attachments";

fn summary(r: &SqliteRow) -> MessageSummary {
    MessageSummary {
        id: r.get("id"),
        account_id: r.get("account_id"),
        folder_id: r.get("folder_id"),
        uid: r.get::<i64, _>("uid") as u32,
        message_id: r.get("message_id"),
        thread_id: r.get("thread_id"),
        subject: r.get("subject"),
        from: r.get::<Option<String>, _>("from_json").and_then(|s| serde_json::from_str(&s).ok()),
        to: from_json(r.get("to_json")),
        cc: from_json(r.get("cc_json")),
        date: r.get("date"),
        preview: r.get("preview"),
        seen: r.get::<i64, _>("seen") != 0,
        flagged: r.get::<i64, _>("flagged") != 0,
        answered: r.get::<i64, _>("answered") != 0,
        has_attachments: r.get::<i64, _>("has_attachments") != 0,
    }
}

fn account(r: &SqliteRow) -> Result<Account> {
    Ok(Account {
        id: r.get("id"),
        provider: Provider::parse(r.get::<&str, _>("provider")).ok_or_else(|| Error::Other("proveedor inválido".into()))?,
        email: r.get("email"),
        display_name: r.get("display_name"),
        username: r.get("username"),
        imap: serde_json::from_str(r.get::<&str, _>("imap_json")).map_err(|e| Error::Other(e.to_string()))?,
        smtp: serde_json::from_str(r.get::<&str, _>("smtp_json")).map_err(|e| Error::Other(e.to_string()))?,
        color: r.get("color"),
        auth_user: r.get("auth_user"),
    })
}

fn people_text(h: &ParsedHeaders) -> String {
    h.from.iter().chain(&h.to).chain(&h.cc).map(EmailAddress::display).collect::<Vec<_>>().join(" ")
}

impl Store {
    pub async fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
            restrict_permissions(dir);
        }
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
        let pool = SqlitePoolOptions::new().max_connections(4).connect_with(opts).await?;
        restrict_permissions(path);
        Self::init(pool).await
    }

    pub async fn open_memory() -> Result<Self> {
        let opts = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
        // A single connection: every new in-memory connection would be a different database.
        let pool = SqlitePoolOptions::new().max_connections(1).connect_with(opts).await?;
        Self::init(pool).await
    }

    async fn init(pool: SqlitePool) -> Result<Self> {
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Store { pool })
    }

    // ---------- accounts ----------

    pub async fn insert_account(&self, a: &Account) -> Result<()> {
        sqlx::query(
            "INSERT INTO accounts (id, provider, email, display_name, username, imap_json, smtp_json, color, auth_user, position)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, (SELECT COALESCE(MAX(position), 0) + 1 FROM accounts))",
        )
        .bind(&a.id)
        .bind(a.provider.as_str())
        .bind(&a.email)
        .bind(&a.display_name)
        .bind(&a.username)
        .bind(json(&a.imap))
        .bind(json(&a.smtp))
        .bind(&a.color)
        .bind(&a.auth_user)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_account_profile(&self, id: &str, display_name: &str, color: &str) -> Result<()> {
        sqlx::query("UPDATE accounts SET display_name = ?, color = ? WHERE id = ?")
            .bind(display_name)
            .bind(color)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn accounts(&self) -> Result<Vec<Account>> {
        sqlx::query("SELECT * FROM accounts ORDER BY position")
            .fetch_all(&self.pool)
            .await?
            .iter()
            .map(account)
            .collect()
    }

    pub async fn account(&self, id: &str) -> Result<Account> {
        let row = sqlx::query("SELECT * FROM accounts WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| Error::NotFound(format!("cuenta {id}")))?;
        account(&row)
    }

    pub async fn account_by_email(&self, email: &str) -> Result<Option<Account>> {
        let row = sqlx::query("SELECT * FROM accounts WHERE lower(email) = lower(?)")
            .bind(email)
            .fetch_optional(&self.pool)
            .await?;
        row.as_ref().map(account).transpose()
    }

    pub async fn delete_account(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM messages_fts WHERE rowid IN (SELECT id FROM messages WHERE account_id = ?)")
            .bind(id)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM accounts WHERE id = ?").bind(id).execute(&self.pool).await?;
        Ok(())
    }

    // ---------- folders ----------

    /// Marks all folders as absent; `upsert_folder` marks the ones still on the server.
    pub async fn begin_folder_refresh(&self, account_id: &str) -> Result<()> {
        sqlx::query("UPDATE folders SET present = 0 WHERE account_id = ?").bind(account_id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn finish_folder_refresh(&self, account_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM messages_fts WHERE rowid IN (SELECT m.id FROM messages m JOIN folders f ON f.id = m.folder_id WHERE f.account_id = ? AND f.present = 0)")
            .bind(account_id)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM folders WHERE account_id = ? AND present = 0").bind(account_id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn upsert_folder(&self, account_id: &str, path: &str, name: &str, role: FolderRole) -> Result<i64> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO folders (account_id, path, name, role, present) VALUES (?, ?, ?, ?, 1)
             ON CONFLICT (account_id, path) DO UPDATE SET name = excluded.name, role = excluded.role, present = 1
             RETURNING id",
        )
        .bind(account_id)
        .bind(path)
        .bind(name)
        .bind(role.as_str())
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    pub async fn folders(&self, account_id: &str) -> Result<Vec<Folder>> {
        let rows = sqlx::query(
            "SELECT f.id, f.account_id, f.path, f.name, f.role,
                    COALESCE(SUM(CASE WHEN m.seen = 0 THEN 1 ELSE 0 END), 0) AS unread,
                    COUNT(m.id) AS total
             FROM folders f LEFT JOIN messages m ON m.folder_id = f.id
             WHERE f.account_id = ?
             GROUP BY f.id",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        let mut folders: Vec<Folder> = rows
            .iter()
            .map(|r| Folder {
                id: r.get("id"),
                account_id: r.get("account_id"),
                path: r.get("path"),
                name: r.get("name"),
                role: FolderRole::parse(r.get("role")),
                unread: r.get("unread"),
                total: r.get("total"),
            })
            .collect();
        folders.sort_by_cached_key(|f| (f.role.sort_key(), f.name.to_lowercase()));
        Ok(folders)
    }

    pub async fn folder(&self, id: i64) -> Result<Folder> {
        let r = sqlx::query("SELECT id, account_id, path, name, role FROM folders WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| Error::NotFound(format!("carpeta {id}")))?;
        Ok(Folder {
            id: r.get("id"),
            account_id: r.get("account_id"),
            path: r.get("path"),
            name: r.get("name"),
            role: FolderRole::parse(r.get("role")),
            unread: 0,
            total: 0,
        })
    }

    pub async fn folder_by_role(&self, account_id: &str, role: FolderRole) -> Result<Option<Folder>> {
        let id: Option<i64> = sqlx::query_scalar("SELECT id FROM folders WHERE account_id = ? AND role = ? ORDER BY id LIMIT 1")
            .bind(account_id)
            .bind(role.as_str())
            .fetch_optional(&self.pool)
            .await?;
        match id {
            Some(id) => Ok(Some(self.folder(id).await?)),
            None => Ok(None),
        }
    }

    pub async fn folder_sync_state(&self, folder_id: i64) -> Result<FolderSync> {
        let r = sqlx::query(
            "SELECT f.uid_validity, f.last_uid, MAX(m.uid) AS max_uid, MIN(m.uid) AS min_uid
             FROM folders f LEFT JOIN messages m ON m.folder_id = f.id WHERE f.id = ? GROUP BY f.id",
        )
        .bind(folder_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(FolderSync {
            uid_validity: r.get::<Option<i64>, _>("uid_validity").map(|v| v as u32),
            last_uid: r.get::<Option<i64>, _>("last_uid").map(|v| v as u32),
            max_uid: r.get::<Option<i64>, _>("max_uid").map(|v| v as u32),
            min_uid: r.get::<Option<i64>, _>("min_uid").map(|v| v as u32),
        })
    }

    /// Stores UIDVALIDITY. When it changed, every cached UID is meaningless and is dropped.
    pub async fn set_uid_validity(&self, folder_id: i64, uid_validity: u32) -> Result<bool> {
        let current = self.folder_sync_state(folder_id).await?.uid_validity;
        let reset = current.is_some_and(|c| c != uid_validity);
        if reset {
            sqlx::query("DELETE FROM messages_fts WHERE rowid IN (SELECT id FROM messages WHERE folder_id = ?)")
                .bind(folder_id)
                .execute(&self.pool)
                .await?;
            sqlx::query("DELETE FROM messages WHERE folder_id = ?").bind(folder_id).execute(&self.pool).await?;
        }
        sqlx::query(if reset {
            "UPDATE folders SET uid_validity = ?, last_uid = NULL WHERE id = ?"
        } else {
            "UPDATE folders SET uid_validity = ? WHERE id = ?"
        })
        .bind(uid_validity as i64)
            .bind(folder_id)
            .execute(&self.pool)
            .await?;
        Ok(reset)
    }

    pub async fn set_last_uid(&self, folder_id: i64, uid: u32) -> Result<()> {
        sqlx::query("UPDATE folders SET last_uid = MAX(COALESCE(last_uid, 0), ?) WHERE id = ?")
            .bind(uid as i64)
            .bind(folder_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ---------- messages ----------

    /// Thread for a new message: inherit the thread of any known ancestor, otherwise use the
    /// root id. Threads that were started by this message's replies are merged into it.
    async fn assign_thread(&self, account_id: &str, h: &ParsedHeaders, fallback: &str) -> Result<String> {
        let ancestors = ancestors(&h.references, h.in_reply_to.as_deref());
        // Closest known ancestor wins (iterate from the parent backwards).
        let mut thread = None;
        for id in ancestors.iter().rev() {
            let t: Option<String> = sqlx::query_scalar("SELECT thread_id FROM messages WHERE account_id = ? AND message_id = ? LIMIT 1")
                .bind(account_id)
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
            if t.is_some() {
                thread = t;
                break;
            }
        }
        // Same message already stored in another folder (Gmail labels, a copy in Sent...).
        if thread.is_none() {
            if let Some(mid) = &h.message_id {
                thread = sqlx::query_scalar("SELECT thread_id FROM messages WHERE account_id = ? AND message_id = ? LIMIT 1")
                    .bind(account_id)
                    .bind(mid)
                    .fetch_optional(&self.pool)
                    .await?;
            }
        }
        let thread = thread.unwrap_or_else(|| thread_root(&h.references, h.in_reply_to.as_deref(), h.message_id.as_deref(), fallback));

        // Replies that arrived before this message may have started threads keyed by our id
        // or by one of our ancestors.
        let mut merge: Vec<&String> = ancestors.iter().collect();
        if let Some(mid) = &h.message_id {
            merge.push(mid);
        }
        for old in merge {
            if *old != thread {
                sqlx::query("UPDATE messages SET thread_id = ? WHERE account_id = ? AND thread_id = ?")
                    .bind(&thread)
                    .bind(account_id)
                    .bind(old)
                    .execute(&self.pool)
                    .await?;
                // A pinned conversation stays pinned under its new id.
                sqlx::query("UPDATE OR IGNORE pinned_threads SET thread_id = ? WHERE account_id = ? AND thread_id = ?")
                    .bind(&thread)
                    .bind(account_id)
                    .bind(old)
                    .execute(&self.pool)
                    .await?;
                sqlx::query("DELETE FROM pinned_threads WHERE account_id = ? AND thread_id = ?")
                    .bind(account_id)
                    .bind(old)
                    .execute(&self.pool)
                    .await?;
            }
        }
        Ok(thread)
    }

    /// Inserts a message header (or refreshes its flags). Returns the row id and whether it is new.
    pub async fn upsert_header(&self, account_id: &str, folder_id: i64, meta: HeaderMeta, h: &ParsedHeaders) -> Result<(i64, bool)> {
        let HeaderMeta { uid, flags, internal_date, size } = meta;
        if let Some(id) = sqlx::query_scalar::<_, i64>("SELECT id FROM messages WHERE folder_id = ? AND uid = ?")
            .bind(folder_id)
            .bind(uid as i64)
            .fetch_optional(&self.pool)
            .await?
        {
            self.set_flags(id, flags).await?;
            return Ok((id, false));
        }

        let thread = self.assign_thread(account_id, h, &format!("{folder_id}:{uid}")).await?;
        let date = h.date.or(internal_date).unwrap_or_else(|| chrono::Utc::now().timestamp());
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO messages (account_id, folder_id, uid, message_id, thread_id, subject, from_json, to_json, cc_json,
                                   date, seen, flagged, answered, has_attachments, in_reply_to, refs, size)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(account_id)
        .bind(folder_id)
        .bind(uid as i64)
        .bind(&h.message_id)
        .bind(&thread)
        .bind(&h.subject)
        .bind(h.from.as_ref().map(json))
        .bind(json(&h.to))
        .bind(json(&h.cc))
        .bind(date)
        .bind(flags.seen)
        .bind(flags.flagged)
        .bind(flags.answered)
        .bind(h.has_attachments)
        .bind(&h.in_reply_to)
        .bind(json(&h.references))
        .bind(size as i64)
        .fetch_one(&self.pool)
        .await?;
        sqlx::query("INSERT INTO messages_fts (rowid, subject, people, body) VALUES (?, ?, ?, '')")
            .bind(id)
            .bind(&h.subject)
            .bind(people_text(h))
            .execute(&self.pool)
            .await?;
        Ok((id, true))
    }

    pub async fn set_flags(&self, id: i64, f: Flags) -> Result<()> {
        sqlx::query("UPDATE messages SET seen = ?, flagged = ?, answered = ? WHERE id = ?")
            .bind(f.seen)
            .bind(f.flagged)
            .bind(f.answered)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_seen(&self, id: i64, seen: bool) -> Result<()> {
        sqlx::query("UPDATE messages SET seen = ? WHERE id = ?").bind(seen).bind(id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn set_flagged(&self, id: i64, flagged: bool) -> Result<()> {
        sqlx::query("UPDATE messages SET flagged = ? WHERE id = ?").bind(flagged).bind(id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn store_body(&self, id: i64, raw: &[u8], preview: &str, body_text: &str, has_attachments: bool) -> Result<()> {
        sqlx::query("UPDATE messages SET raw = ?, preview = ?, has_attachments = ? WHERE id = ?")
            .bind(raw)
            .bind(preview)
            .bind(has_attachments)
            .bind(id)
            .execute(&self.pool)
            .await?;
        sqlx::query("UPDATE messages_fts SET body = ? WHERE rowid = ?").bind(body_text).bind(id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn raw(&self, id: i64) -> Result<Option<Vec<u8>>> {
        Ok(sqlx::query_scalar("SELECT raw FROM messages WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .flatten())
    }

    /// Ids of the newest messages in a folder whose body hasn't been downloaded yet.
    pub async fn missing_bodies(&self, folder_id: i64, limit: i64, max_size: i64) -> Result<Vec<(i64, u32)>> {
        let rows = sqlx::query("SELECT id, uid FROM messages WHERE folder_id = ? AND raw IS NULL AND size <= ? ORDER BY date DESC LIMIT ?")
            .bind(folder_id)
            .bind(max_size)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter().map(|r| (r.get("id"), r.get::<i64, _>("uid") as u32)).collect())
    }

    pub async fn uids_in_range(&self, folder_id: i64, min: u32, max: u32) -> Result<Vec<(i64, u32)>> {
        let rows = sqlx::query("SELECT id, uid FROM messages WHERE folder_id = ? AND uid BETWEEN ? AND ?")
            .bind(folder_id)
            .bind(min as i64)
            .bind(max as i64)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter().map(|r| (r.get("id"), r.get::<i64, _>("uid") as u32)).collect())
    }

    pub async fn delete_message(&self, id: i64) -> Result<()> {
        sqlx::query("DELETE FROM messages_fts WHERE rowid = ?").bind(id).execute(&self.pool).await?;
        sqlx::query("DELETE FROM messages WHERE id = ?").bind(id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn message(&self, id: i64) -> Result<MessageSummary> {
        let r = sqlx::query(&format!("SELECT {SUMMARY_COLS} FROM messages WHERE id = ?"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| Error::NotFound(format!("mensaje {id}")))?;
        Ok(summary(&r))
    }

    pub async fn location(&self, id: i64) -> Result<MessageLocation> {
        let r = sqlx::query(
            "SELECT m.id, m.account_id, m.folder_id, m.uid, f.path, f.role FROM messages m JOIN folders f ON f.id = m.folder_id WHERE m.id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("mensaje {id}")))?;
        Ok(MessageLocation {
            id: r.get("id"),
            account_id: r.get("account_id"),
            folder_id: r.get("folder_id"),
            folder_path: r.get("path"),
            folder_role: FolderRole::parse(r.get("role")),
            uid: r.get::<i64, _>("uid") as u32,
        })
    }

    pub async fn search_by_message_id(&self, account_id: &str, message_id: &str) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar("SELECT id FROM messages WHERE account_id = ? AND message_id = ?")
            .bind(account_id)
            .bind(crate::threading::normalize_id(message_id))
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn message_threading(&self, id: i64) -> Result<(Option<String>, Vec<String>)> {
        let r = sqlx::query("SELECT message_id, refs FROM messages WHERE id = ?").bind(id).fetch_one(&self.pool).await?;
        Ok((r.get("message_id"), from_json(r.get("refs"))))
    }

    pub async fn messages(&self, folder_id: i64, limit: i64, offset: i64) -> Result<Vec<MessageSummary>> {
        let rows = sqlx::query(&format!("SELECT {SUMMARY_COLS} FROM messages WHERE folder_id = ? ORDER BY date DESC LIMIT ? OFFSET ?"))
            .bind(folder_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter().map(summary).collect())
    }

    /// Conversations that have at least one message in any of `folder_ids` (several folders
    /// make a unified inbox). Counts include messages of the thread in other folders, e.g.
    /// the user's own replies in Sent.
    pub async fn threads(&self, folder_ids: &[i64], limit: i64, offset: i64) -> Result<Vec<Thread>> {
        if folder_ids.is_empty() {
            return Ok(vec![]);
        }
        let placeholders = vec!["?"; folder_ids.len()].join(",");
        let sql = format!(
            "SELECT m.account_id, m.thread_id, MAX(m.date) AS last_date, (p.thread_id IS NOT NULL) AS pinned
             FROM messages m LEFT JOIN pinned_threads p ON p.account_id = m.account_id AND p.thread_id = m.thread_id
             WHERE m.folder_id IN ({placeholders})
             GROUP BY m.account_id, m.thread_id ORDER BY pinned DESC, last_date DESC LIMIT ? OFFSET ?"
        );
        let mut q = sqlx::query(&sql);
        for id in folder_ids {
            q = q.bind(id);
        }
        let keys = q.bind(limit).bind(offset).fetch_all(&self.pool).await?;

        let mut out = Vec::with_capacity(keys.len());
        for k in &keys {
            let account_id: String = k.get("account_id");
            let thread_id: String = k.get("thread_id");
            let pinned: bool = k.get("pinned");
            let msgs = self.thread_summaries(&account_id, &thread_id).await?;
            let in_folder: Vec<&MessageSummary> = msgs.iter().filter(|m| folder_ids.contains(&m.folder_id)).collect();
            let Some(latest) = in_folder.iter().max_by_key(|m| m.date) else { continue };
            let mut participants: Vec<EmailAddress> = Vec::new();
            for m in &msgs {
                if let Some(f) = &m.from {
                    if !participants.iter().any(|p| p.email.eq_ignore_ascii_case(&f.email)) {
                        participants.push(f.clone());
                    }
                }
            }
            let first_subject = msgs.first().map(|m| m.subject.clone()).unwrap_or_default();
            out.push(Thread {
                thread_id,
                subject: if first_subject.is_empty() { latest.subject.clone() } else { first_subject },
                participants,
                recipients: latest.to.iter().chain(&latest.cc).cloned().collect(),
                last_date: latest.date,
                message_count: msgs.len(),
                unread_count: msgs.iter().filter(|m| !m.seen).count(),
                flagged: msgs.iter().any(|m| m.flagged),
                has_attachments: msgs.iter().any(|m| m.has_attachments),
                preview: latest.preview.clone(),
                latest_id: latest.id,
                account_id,
                pinned,
            });
        }
        Ok(out)
    }

    /// All messages of a thread across folders, oldest first, with duplicates (same
    /// Message-ID in several folders/labels) collapsed. Trash and Junk copies are only used
    /// when there is no other copy.
    pub async fn thread_summaries(&self, account_id: &str, thread_id: &str) -> Result<Vec<MessageSummary>> {
        let rows = sqlx::query(&format!(
            "SELECT {SUMMARY_COLS}, (SELECT role FROM folders WHERE id = messages.folder_id) AS role
             FROM messages WHERE account_id = ? AND thread_id = ? ORDER BY date ASC, id ASC"
        ))
        .bind(account_id)
        .bind(thread_id)
        .fetch_all(&self.pool)
        .await?;
        let rank = |role: &str| match FolderRole::parse(role) {
            FolderRole::Inbox | FolderRole::Sent => 0,
            FolderRole::Trash | FolderRole::Junk => 3,
            FolderRole::All => 2,
            _ => 1,
        };
        let mut by_key: HashMap<String, (usize, MessageSummary, u8)> = HashMap::new();
        for (i, r) in rows.iter().enumerate() {
            let s = summary(r);
            let role: String = r.get("role");
            let key = s.message_id.clone().unwrap_or_else(|| format!("row:{}", s.id));
            let rk = rank(&role);
            match by_key.get_mut(&key) {
                Some(existing) if existing.2 <= rk => {
                    // Keep the better copy but merge unread state: unread if any copy is unread.
                    existing.1.seen &= s.seen;
                }
                Some(existing) => {
                    let seen = existing.1.seen && s.seen;
                    *existing = (existing.0, s, rk);
                    existing.1.seen = seen;
                }
                None => {
                    by_key.insert(key, (i, s, rk));
                }
            }
        }
        let mut v: Vec<(usize, MessageSummary, u8)> = by_key.into_values().collect();
        v.sort_by_key(|(i, s, _)| (s.date, *i));
        Ok(v.into_iter().map(|(_, s, _)| s).collect())
    }

    pub async fn set_thread_pinned(&self, account_id: &str, thread_id: &str, pinned: bool) -> Result<()> {
        if pinned {
            sqlx::query("INSERT OR IGNORE INTO pinned_threads (account_id, thread_id, pinned_at) VALUES (?, ?, ?)")
                .bind(account_id)
                .bind(thread_id)
                .bind(chrono::Utc::now().timestamp())
                .execute(&self.pool)
                .await?;
        } else {
            sqlx::query("DELETE FROM pinned_threads WHERE account_id = ? AND thread_id = ?")
                .bind(account_id)
                .bind(thread_id)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    /// All row ids (every folder copy) of the messages in a thread.
    pub async fn thread_row_ids(&self, account_id: &str, thread_id: &str) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar("SELECT id FROM messages WHERE account_id = ? AND thread_id = ?")
            .bind(account_id)
            .bind(thread_id)
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn search(&self, query: &str, account_id: Option<&str>, limit: i64) -> Result<Vec<MessageSummary>> {
        // Quote each term so user input can't use FTS operators; prefix-match the last one.
        let terms: Vec<String> = query
            .split_whitespace()
            .map(|t| format!("\"{}\"", t.replace('"', "")))
            .collect();
        if terms.is_empty() {
            return Ok(vec![]);
        }
        let fts = format!("{}*", terms.join(" "));
        let rows = sqlx::query(&format!(
            "SELECT {SUMMARY_COLS} FROM messages WHERE id IN (SELECT rowid FROM messages_fts WHERE messages_fts MATCH ?)
             AND (? IS NULL OR account_id = ?) ORDER BY date DESC LIMIT ?"
        ))
        .bind(fts)
        .bind(account_id)
        .bind(account_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(summary).collect())
    }

    /// Recipients seen in mail, for composer autocomplete.
    pub async fn contacts(&self, prefix: &str, limit: usize) -> Result<Vec<EmailAddress>> {
        let like = format!("%{}%", prefix.replace('%', "").to_lowercase());
        let rows: Vec<(Option<String>, String, String)> = sqlx::query_as(
            "SELECT from_json, to_json, cc_json FROM messages
             WHERE lower(from_json) LIKE ?1 OR lower(to_json) LIKE ?1 OR lower(cc_json) LIKE ?1
             ORDER BY date DESC LIMIT 300",
        )
        .bind(&like)
        .fetch_all(&self.pool)
        .await?;
        let p = prefix.to_lowercase();
        let mut seen: HashMap<String, (EmailAddress, usize)> = HashMap::new();
        for (from, to, cc) in rows {
            let mut list: Vec<EmailAddress> = from_json(Some(to));
            list.extend(from_json::<Vec<EmailAddress>>(Some(cc)));
            if let Some(f) = from.and_then(|f| serde_json::from_str::<EmailAddress>(&f).ok()) {
                list.push(f);
            }
            for a in list {
                let matches = a.email.to_lowercase().contains(&p)
                    || a.name.as_ref().is_some_and(|n| n.to_lowercase().contains(&p));
                if matches {
                    let e = seen.entry(a.email.to_lowercase()).or_insert((a.clone(), 0));
                    e.1 += 1;
                    if e.0.name.is_none() && a.name.is_some() {
                        e.0.name = a.name;
                    }
                }
            }
        }
        let mut v: Vec<(EmailAddress, usize)> = seen.into_values().collect();
        v.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        Ok(v.into_iter().take(limit).map(|(a, _)| a).collect())
    }

    pub async fn unread_inbox_count(&self) -> Result<i64> {
        Ok(sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages m JOIN folders f ON f.id = m.folder_id WHERE f.role = 'inbox' AND m.seen = 0",
        )
        .fetch_one(&self.pool)
        .await?)
    }

    // ---------- signatures ----------

    pub async fn signatures(&self) -> Result<Vec<Signature>> {
        let rows = sqlx::query("SELECT * FROM signatures ORDER BY name").fetch_all(&self.pool).await?;
        Ok(rows
            .iter()
            .map(|r| Signature {
                id: r.get("id"),
                account_id: r.get("account_id"),
                name: r.get("name"),
                is_html: r.get::<i64, _>("is_html") != 0,
                body: r.get("body"),
                is_default: r.get::<i64, _>("is_default") != 0,
            })
            .collect())
    }

    pub async fn save_signature(&self, s: &Signature) -> Result<Signature> {
        let mut s = s.clone();
        if s.id.is_empty() {
            s.id = uuid::Uuid::new_v4().to_string();
        }
        if s.is_html {
            s.body = sanitize_signature_html(&s.body);
        }
        if s.is_default {
            sqlx::query("UPDATE signatures SET is_default = 0 WHERE account_id IS ?")
                .bind(&s.account_id)
                .execute(&self.pool)
                .await?;
        }
        sqlx::query(
            "INSERT INTO signatures (id, account_id, name, is_html, body, is_default) VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT (id) DO UPDATE SET account_id = excluded.account_id, name = excluded.name,
                 is_html = excluded.is_html, body = excluded.body, is_default = excluded.is_default",
        )
        .bind(&s.id)
        .bind(&s.account_id)
        .bind(&s.name)
        .bind(s.is_html)
        .bind(&s.body)
        .bind(s.is_default)
        .execute(&self.pool)
        .await?;
        Ok(s)
    }

    pub async fn delete_signature(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM signatures WHERE id = ?").bind(id).execute(&self.pool).await?;
        Ok(())
    }

    // ---------- settings ----------

    pub async fn settings(&self) -> Result<Settings> {
        let v: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = 'app'")
            .fetch_optional(&self.pool)
            .await?;
        Ok(v.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default())
    }

    pub async fn save_settings(&self, s: &Settings) -> Result<()> {
        sqlx::query("INSERT INTO settings (key, value) VALUES ('app', ?) ON CONFLICT (key) DO UPDATE SET value = excluded.value")
            .bind(json(s))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn trust_sender(&self, email: &str) -> Result<()> {
        sqlx::query("INSERT OR IGNORE INTO trusted_senders (email) VALUES (lower(?))")
            .bind(email)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn is_trusted(&self, email: &str) -> Result<bool> {
        Ok(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM trusted_senders WHERE email = lower(?)")
            .bind(email)
            .fetch_one(&self.pool)
            .await?
            > 0)
    }
}

/// The cache holds private mail: make it readable by the current user only.
fn restrict_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if path.is_dir() { 0o700 } else { 0o600 };
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
    }
    #[cfg(not(unix))]
    let _ = path; // On Windows/macOS the per-user app data directory is already private.
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acct() -> Account {
        let s = ServerConfig { host: "imap.x".into(), port: 993, security: Security::Tls };
        Account {
            id: "acc".into(),
            provider: Provider::Imap,
            email: "yo@x.com".into(),
            display_name: "Yo".into(),
            username: "yo@x.com".into(),
            imap: s.clone(),
            smtp: s,
            color: "#123".into(),
            auth_user: None,
        }
    }

    fn hdr(id: &str, irt: Option<&str>, refs: &[&str], from: &str, date: i64) -> ParsedHeaders {
        ParsedHeaders {
            message_id: Some(id.into()),
            subject: "Asado".into(),
            from: Some(EmailAddress { name: Some(from.into()), email: format!("{}@x.com", from.to_lowercase()) }),
            to: vec![EmailAddress { name: None, email: "yo@x.com".into() }],
            in_reply_to: irt.map(Into::into),
            references: refs.iter().map(|s| s.to_string()).collect(),
            date: Some(date),
            ..Default::default()
        }
    }

    fn meta(uid: u32, flags: Flags) -> HeaderMeta {
        HeaderMeta { uid, flags, internal_date: None, size: 10 }
    }

    async fn setup() -> (Store, i64, i64) {
        let s = Store::open_memory().await.unwrap();
        s.insert_account(&acct()).await.unwrap();
        let inbox = s.upsert_folder("acc", "INBOX", "Recibidos", FolderRole::Inbox).await.unwrap();
        let sent = s.upsert_folder("acc", "Sent", "Enviados", FolderRole::Sent).await.unwrap();
        (s, inbox, sent)
    }

    #[tokio::test]
    async fn threads_across_folders_and_out_of_order() {
        let (s, inbox, sent) = setup().await;
        let f = Flags::default();
        // Reply arrives first, referencing only its parent (Outlook style).
        s.upsert_header("acc", inbox, meta(3, f), &hdr("c", Some("b"), &[], "Ana", 300)).await.unwrap();
        // Our own reply in Sent, with full References.
        s.upsert_header("acc", sent, meta(1, Flags { seen: true, ..f }), &hdr("b", Some("a"), &["a"], "Yo", 200)).await.unwrap();
        // The original message.
        s.upsert_header("acc", inbox, meta(1, f), &hdr("a", None, &[], "Ana", 100)).await.unwrap();
        // Unrelated message.
        s.upsert_header("acc", inbox, meta(2, f), &hdr("z", None, &[], "Luis", 150)).await.unwrap();

        let threads = s.threads(&[inbox], 50, 0).await.unwrap();
        assert_eq!(threads.len(), 2);
        let t = &threads[0];
        assert_eq!(t.message_count, 3);
        assert_eq!(t.unread_count, 2);
        assert_eq!(t.last_date, 300);
        let msgs = s.thread_summaries("acc", &t.thread_id).await.unwrap();
        let ids: Vec<_> = msgs.iter().map(|m| m.message_id.clone().unwrap()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[tokio::test]
    async fn pinned_threads_first_and_survive_merge() {
        let (s, inbox, _) = setup().await;
        let f = Flags::default();
        // A reply whose parent hasn't arrived yet starts a thread keyed by the parent's id.
        s.upsert_header("acc", inbox, meta(1, f), &hdr("b", Some("a"), &[], "Ana", 100)).await.unwrap();
        s.upsert_header("acc", inbox, meta(2, f), &hdr("z", None, &[], "Luis", 500)).await.unwrap();
        let old = s.threads(&[inbox], 50, 0).await.unwrap().into_iter().find(|t| t.message_count == 1 && t.last_date == 100).unwrap();
        s.set_thread_pinned("acc", &old.thread_id, true).await.unwrap();

        let threads = s.threads(&[inbox], 50, 0).await.unwrap();
        assert!(threads[0].pinned && threads[0].last_date == 100);
        assert!(!threads[1].pinned);

        // The original arrives with a grandparent reference: the thread gets merged/renamed.
        s.upsert_header("acc", inbox, meta(3, f), &hdr("a", Some("root"), &["root"], "Ana", 50)).await.unwrap();
        let threads = s.threads(&[inbox], 50, 0).await.unwrap();
        assert!(threads[0].pinned);
        assert_eq!(threads[0].message_count, 2);

        s.set_thread_pinned("acc", &threads[0].thread_id, false).await.unwrap();
        assert!(!s.threads(&[inbox], 50, 0).await.unwrap().iter().any(|t| t.pinned));
    }

    #[tokio::test]
    async fn duplicates_collapse_and_flags_update() {
        let (s, inbox, _) = setup().await;
        let all = s.upsert_folder("acc", "[Gmail]/All Mail", "Todos", FolderRole::All).await.unwrap();
        let (id, new) = s.upsert_header("acc", inbox, meta(1, Flags::default()), &hdr("a", None, &[], "Ana", 1)).await.unwrap();
        assert!(new);
        s.upsert_header("acc", all, meta(9, Flags::default()), &hdr("a", None, &[], "Ana", 1)).await.unwrap();
        let (id2, new2) = s
            .upsert_header("acc", inbox, meta(1, Flags { seen: true, ..Default::default() }), &hdr("a", None, &[], "Ana", 1))
            .await
            .unwrap();
        assert_eq!(id, id2);
        assert!(!new2);
        let t = s.thread_summaries("acc", "a").await.unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].folder_id, inbox);
    }

    #[tokio::test]
    async fn uid_validity_reset_and_search() {
        let (s, inbox, _) = setup().await;
        assert!(!s.set_uid_validity(inbox, 7).await.unwrap());
        let (id, _) = s.upsert_header("acc", inbox, meta(1, Flags::default()), &hdr("a", None, &[], "Ana", 1)).await.unwrap();
        s.store_body(id, b"raw", "preview", "carne a la parrilla", false).await.unwrap();
        assert_eq!(s.search("parri", None, 10).await.unwrap().len(), 1);
        assert_eq!(s.search("asado ana", Some("acc"), 10).await.unwrap().len(), 1);
        assert_eq!(s.search("\"OR", None, 10).await.unwrap().len(), 0);
        assert_eq!(s.unread_inbox_count().await.unwrap(), 1);
        assert!(s.set_uid_validity(inbox, 8).await.unwrap());
        assert_eq!(s.messages(inbox, 10, 0).await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn contacts_signatures_settings() {
        let (s, inbox, _) = setup().await;
        s.upsert_header("acc", inbox, meta(1, Flags::default()), &hdr("a", None, &[], "Ana", 1)).await.unwrap();
        let c = s.contacts("ana", 5).await.unwrap();
        assert_eq!(c[0].email, "ana@x.com");

        let sig = s
            .save_signature(&Signature {
                id: String::new(),
                account_id: Some("acc".into()),
                name: "Trabajo".into(),
                is_html: true,
                body: "<b>Yo</b><script>x()</script>".into(),
                is_default: true,
            })
            .await
            .unwrap();
        assert!(!sig.body.contains("script"));
        assert_eq!(s.signatures().await.unwrap().len(), 1);

        let mut st = s.settings().await.unwrap();
        assert!(st.close_to_tray);
        st.chat_view = true;
        s.save_settings(&st).await.unwrap();
        assert!(s.settings().await.unwrap().chat_view);

        // Shared mailbox accounts keep their delegate user.
        let mut shared = acct();
        shared.id = "shared".into();
        shared.email = "ventas@x.com".into();
        shared.username = "ventas@x.com".into();
        shared.auth_user = Some("yo@x.com".into());
        s.insert_account(&shared).await.unwrap();
        assert_eq!(s.account("shared").await.unwrap().auth_user.as_deref(), Some("yo@x.com"));
        assert_eq!(s.account("acc").await.unwrap().auth_user, None);
        s.delete_account("shared").await.unwrap();

        // Settings saved by an older version (no `sidebarCollapsed`) must still load as saved.
        sqlx::query("UPDATE settings SET value = ? WHERE key = 'app'")
            .bind(r#"{"closeToTray":false,"notifications":true,"doNotDisturb":false,"chatView":true,"theme":"dark","loadRemoteImages":false,"startMinimized":false}"#)
            .execute(&s.pool)
            .await
            .unwrap();
        let old = s.settings().await.unwrap();
        assert!(!old.close_to_tray && old.chat_view && old.theme == "dark" && !old.sidebar_collapsed && old.new_mail_sound == "corneta-suave" && old.hidden_folders.is_empty());
        assert!(old.custom_sound_name.is_none() && old.open_tabs.is_empty());
        assert!(old.chat_send_key == "enter" && old.composer_send_key == "ctrlEnter");
        assert!(old.signature_defaults.is_empty() && old.account_sounds.is_empty());

        s.delete_account("acc").await.unwrap();
        assert!(s.accounts().await.unwrap().is_empty());
        assert!(s.signatures().await.unwrap().is_empty());
    }
}
