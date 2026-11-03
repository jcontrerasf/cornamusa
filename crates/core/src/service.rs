//! The mail service: owns the store, keeps one background worker per account (sync + IMAP
//! IDLE) and implements every user action. The UI shell only forwards calls and events.

use crate::error::{Error, Result};
use crate::imap::{self, Credential, Session};
use crate::mime::{self, parse_full, parse_headers};
use crate::model::*;
use crate::oauth;
use crate::providers::{self, DetectedConfig};
use crate::secrets::{SecretKind, Secrets};
use crate::store::{Flags, Store};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

/// Messages fetched the first time a folder is synced.
const INITIAL_WINDOW: u32 = 400;
/// Bodies downloaded ahead of time per folder sync (for previews and offline reading).
const PREFETCH_BODIES: i64 = 40;
const PREFETCH_MAX_SIZE: i64 = 512 * 1024;
/// Time an account stays in IDLE before a full resync (which also refreshes OAuth tokens).
const IDLE_TIMEOUT: Duration = Duration::from_secs(25 * 60);
/// IDLE is re-issued on the same connection this often (RFC 2177 asks for less than 29 minutes):
/// the traffic keeps routers/NATs from dropping the idle connection, and a dead one shows up in
/// minutes instead of at the next full resync.
const IDLE_REFRESH: Duration = Duration::from_secs(5 * 60);

/// Messages to act on, grouped by (folder id, folder path, role).
type ByFolder = BTreeMap<(i64, String, FolderRole), Vec<(i64, u32)>>;
/// Uncached message bodies, grouped by account then folder path.
type MissingBodies = BTreeMap<String, Vec<(String, Vec<(i64, u32)>)>>;

const ACCOUNT_COLORS: [&str; 8] = ["#6366f1", "#ec4899", "#14b8a6", "#f59e0b", "#8b5cf6", "#ef4444", "#22c55e", "#0ea5e9"];

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Event {
    AccountsChanged,
    #[serde(rename_all = "camelCase")]
    MailboxChanged { account_id: String },
    NewMail(NewMailEvent),
    #[serde(rename_all = "camelCase")]
    SyncStatus { account_id: String, syncing: bool, error: Option<String> },
    #[serde(rename_all = "camelCase")]
    UnreadCount { count: i64 },
}

pub trait EventSink: Send + Sync + 'static {
    fn emit(&self, event: Event);
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PasswordAccountRequest {
    pub email: String,
    pub display_name: String,
    pub username: Option<String>,
    pub password: String,
    pub imap: ServerConfig,
    pub smtp: ServerConfig,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MoveTarget {
    Inbox,
    Archive,
    Junk,
    Trash,
}

pub struct MailService {
    store: Store,
    sink: Arc<dyn EventSink>,
    secrets: Secrets,
    /// Access tokens in memory only: account id → (token, expires_at).
    tokens: Mutex<HashMap<String, (String, i64)>>,
    workers: std::sync::Mutex<HashMap<String, JoinHandle<()>>>,
    /// Serializes syncs of the same account (worker, IDLE wake-ups and manual refresh).
    sync_locks: std::sync::Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl MailService {
    pub fn new(store: Store, sink: Arc<dyn EventSink>) -> Arc<Self> {
        Self::with_secrets(store, sink, Secrets::Keyring)
    }

    pub fn with_secrets(store: Store, sink: Arc<dyn EventSink>, secrets: Secrets) -> Arc<Self> {
        Arc::new(MailService {
            store,
            sink,
            secrets,
            tokens: Mutex::new(HashMap::new()),
            workers: std::sync::Mutex::new(HashMap::new()),
            sync_locks: std::sync::Mutex::new(HashMap::new()),
        })
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    fn emit(&self, e: Event) {
        self.sink.emit(e);
    }

    async fn mailbox_changed(&self, account_id: &str) {
        self.emit(Event::MailboxChanged { account_id: account_id.to_string() });
        if let Ok(count) = self.store.unread_inbox_count().await {
            self.emit(Event::UnreadCount { count });
        }
    }

    fn sync_lock(&self, account_id: &str) -> Arc<Mutex<()>> {
        self.sync_locks.lock().unwrap().entry(account_id.to_string()).or_default().clone()
    }

    // ---------------------------------------------------------------- credentials

    async fn credential(&self, account: &Account) -> Result<Credential> {
        if !account.provider.uses_oauth() {
            return Ok(Credential::Password {
                user: account.username.clone(),
                password: self.secrets.load(&account.id, SecretKind::Password).await?,
            });
        }
        let now = chrono::Utc::now().timestamp();
        let mut tokens = self.tokens.lock().await;
        if let Some((token, exp)) = tokens.get(&account.id) {
            if *exp > now + 120 {
                return Ok(Credential::OAuth { user: account.username.clone(), access_token: token.clone() });
            }
        }
        let refresh = self.secrets.load(&account.id, SecretKind::RefreshToken).await?;
        let fresh = oauth::refresh(account.provider, &refresh).await.map_err(|e| {
            Error::Auth(format!("La sesión de {} expiró; vuelve a conectar la cuenta. ({e})", account.email))
        })?;
        if let Some(rotated) = &fresh.refresh_token {
            // Microsoft rotates refresh tokens.
            self.secrets.store(&account.id, SecretKind::RefreshToken, rotated).await?;
        }
        tokens.insert(account.id.clone(), (fresh.access_token.clone(), fresh.expires_at));
        Ok(Credential::OAuth { user: account.username.clone(), access_token: fresh.access_token })
    }

    async fn session(&self, account: &Account) -> Result<Session> {
        let cred = self.credential(account).await?;
        imap::connect(&account.imap, &cred).await
    }

    // ---------------------------------------------------------------- accounts

    pub async fn detect(&self, email: &str) -> DetectedConfig {
        providers::detect(email).await
    }

    async fn next_color(&self) -> String {
        let n = self.store.accounts().await.map(|a| a.len()).unwrap_or(0);
        ACCOUNT_COLORS[n % ACCOUNT_COLORS.len()].to_string()
    }

    pub async fn add_oauth_account(
        self: &Arc<Self>,
        provider: Provider,
        login_hint: Option<String>,
        open_browser: impl FnOnce(&str) -> Result<()>,
    ) -> Result<Account> {
        let preset = providers::preset(provider).ok_or_else(|| Error::Config("Proveedor sin OAuth".into()))?;
        let (tokens, identity) = oauth::authorize(provider, login_hint.as_deref(), open_browser).await?;
        let email = identity
            .email
            .or(login_hint)
            .ok_or_else(|| Error::Auth("El proveedor no devolvió la dirección de correo".into()))?;
        let refresh = tokens
            .refresh_token
            .clone()
            .ok_or_else(|| Error::Auth("El proveedor no entregó un refresh token".into()))?;
        if self.store.account_by_email(&email).await?.is_some() {
            return Err(Error::Config(format!("La cuenta {email} ya está configurada")));
        }
        let account = Account {
            id: uuid::Uuid::new_v4().to_string(),
            provider,
            display_name: identity.name.unwrap_or_else(|| email.split('@').next().unwrap_or(&email).to_string()),
            username: email.clone(),
            email,
            imap: preset.imap,
            smtp: preset.smtp,
            color: self.next_color().await,
            auth_user: None,
        };
        // Verify IMAP access before persisting anything.
        let cred = Credential::OAuth { user: account.username.clone(), access_token: tokens.access_token.clone() };
        imap::connect(&account.imap, &cred).await?.logout().await.ok();

        self.secrets.store(&account.id, SecretKind::RefreshToken, &refresh).await?;
        self.tokens.lock().await.insert(account.id.clone(), (tokens.access_token, tokens.expires_at));
        self.store.insert_account(&account).await?;
        self.emit(Event::AccountsChanged);
        self.start_worker(&account.id);
        Ok(account)
    }

    /// Adds a Microsoft 365 shared mailbox the signed-in user has *Full Access* to. IMAP
    /// authenticates as the mailbox (XOAUTH2 with the user's token); SMTP as the user, sending
    /// with the mailbox as `From`, which needs *Send As*. Each shared mailbox gets its own
    /// sign-in and refresh token, so rotated tokens never have to be shared between accounts.
    pub async fn add_shared_mailbox(
        self: &Arc<Self>,
        shared_email: String,
        login_hint: Option<String>,
        open_browser: impl FnOnce(&str) -> Result<()>,
    ) -> Result<Account> {
        let provider = Provider::Outlook;
        let shared = shared_email.trim().to_lowercase();
        if !shared.contains('@') {
            return Err(Error::Config("Escribe la dirección del buzón compartido".into()));
        }
        if self.store.account_by_email(&shared).await?.is_some() {
            return Err(Error::Config(format!("La cuenta {shared} ya está configurada")));
        }
        let preset = providers::preset(provider).ok_or_else(|| Error::Config("Proveedor sin OAuth".into()))?;
        let (tokens, identity) = oauth::authorize(provider, login_hint.as_deref(), open_browser).await?;
        let user = identity
            .email
            .or(login_hint)
            .ok_or_else(|| Error::Auth("Microsoft no devolvió tu dirección de correo".into()))?;
        if user.eq_ignore_ascii_case(&shared) {
            return Err(Error::Config(
                "Esa es tu propia cuenta: agrégala como cuenta normal de Outlook".into(),
            ));
        }
        let refresh = tokens
            .refresh_token
            .clone()
            .ok_or_else(|| Error::Auth("Microsoft no entregó un refresh token".into()))?;
        let account = Account {
            id: uuid::Uuid::new_v4().to_string(),
            provider,
            display_name: shared.split('@').next().unwrap_or(&shared).to_string(),
            username: shared.clone(),
            email: shared.clone(),
            imap: preset.imap,
            smtp: preset.smtp,
            color: self.next_color().await,
            auth_user: Some(user.clone()),
        };
        let imap_cred = Credential::OAuth { user: shared.clone(), access_token: tokens.access_token.clone() };
        match imap::connect(&account.imap, &imap_cred).await {
            Ok(mut session) => {
                session.logout().await.ok();
            }
            Err(Error::Auth(e)) => {
                return Err(Error::Auth(format!(
                    "Tu usuario {user} no tiene acceso al buzón {shared}. Pide al administrador el permiso \"Full Access\" (acceso total) sobre ese buzón. ({e})"
                )))
            }
            Err(e) => return Err(e),
        }
        let smtp_cred = Credential::OAuth { user: user.clone(), access_token: tokens.access_token.clone() };
        crate::smtp::verify(&account.smtp, &smtp_cred).await?;

        self.secrets.store(&account.id, SecretKind::RefreshToken, &refresh).await?;
        self.tokens.lock().await.insert(account.id.clone(), (tokens.access_token, tokens.expires_at));
        self.store.insert_account(&account).await?;
        self.emit(Event::AccountsChanged);
        self.start_worker(&account.id);
        Ok(account)
    }

    pub async fn add_password_account(self: &Arc<Self>, req: PasswordAccountRequest) -> Result<Account> {
        let email = req.email.trim().to_string();
        if self.store.account_by_email(&email).await?.is_some() {
            return Err(Error::Config(format!("La cuenta {email} ya está configurada")));
        }
        let username = req.username.filter(|u| !u.trim().is_empty()).unwrap_or_else(|| email.clone());
        let cred = Credential::Password { user: username.clone(), password: req.password.clone() };
        imap::connect(&req.imap, &cred).await?.logout().await.ok();
        crate::smtp::verify(&req.smtp, &cred).await?;

        let account = Account {
            id: uuid::Uuid::new_v4().to_string(),
            provider: Provider::Imap,
            display_name: if req.display_name.trim().is_empty() { email.clone() } else { req.display_name },
            email,
            username,
            imap: req.imap,
            smtp: req.smtp,
            color: self.next_color().await,
            auth_user: None,
        };
        self.secrets.store(&account.id, SecretKind::Password, &req.password).await?;
        self.store.insert_account(&account).await?;
        self.emit(Event::AccountsChanged);
        self.start_worker(&account.id);
        Ok(account)
    }

    pub async fn remove_account(&self, id: &str) -> Result<()> {
        if let Some(h) = self.workers.lock().unwrap().remove(id) {
            h.abort();
        }
        self.tokens.lock().await.remove(id);
        self.secrets.delete(id).await?;
        self.store.delete_account(id).await?;
        self.emit(Event::AccountsChanged);
        if let Ok(count) = self.store.unread_inbox_count().await {
            self.emit(Event::UnreadCount { count });
        }
        Ok(())
    }

    pub async fn update_account(&self, id: &str, display_name: &str, color: &str) -> Result<()> {
        self.store.update_account_profile(id, display_name, color).await?;
        self.emit(Event::AccountsChanged);
        Ok(())
    }

    // ---------------------------------------------------------------- background workers

    pub async fn start_all(self: &Arc<Self>) -> Result<()> {
        for a in self.store.accounts().await? {
            self.start_worker(&a.id);
        }
        Ok(())
    }

    pub fn stop_all(&self) {
        for (_, h) in self.workers.lock().unwrap().drain() {
            h.abort();
        }
    }

    fn start_worker(self: &Arc<Self>, account_id: &str) {
        let me = self.clone();
        let id = account_id.to_string();
        // Supervises the worker: a panic in it (a bug) would otherwise stop the account from
        // receiving mail until the app restarts.
        let handle = tokio::spawn(async move {
            use futures::FutureExt;
            loop {
                if std::panic::AssertUnwindSafe(me.clone().worker(id.clone())).catch_unwind().await.is_ok() {
                    return;
                }
                tracing::error!(account = %id, "worker panicked, restarting");
                tokio::time::sleep(Duration::from_secs(15)).await;
            }
        });
        if let Some(old) = self.workers.lock().unwrap().insert(account_id.to_string(), handle) {
            old.abort();
        }
    }

    async fn worker(self: Arc<Self>, account_id: String) {
        let mut backoff = 15u64;
        loop {
            match self.sync_account(&account_id).await {
                Ok(()) => backoff = 15,
                Err(e) => {
                    tracing::warn!(account = %account_id, "sync failed: {e}");
                    tokio::time::sleep(Duration::from_secs(backoff)).await;
                    backoff = (backoff * 2).min(600);
                    continue;
                }
            }
            if let Err(e) = self.idle_inbox(&account_id).await {
                tracing::info!(account = %account_id, "idle ended: {e}");
                tokio::time::sleep(Duration::from_secs(20)).await;
            }
        }
    }

    /// Holds a dedicated connection in IDLE on the inbox, re-issued every IDLE_REFRESH; returns
    /// after IDLE_TIMEOUT so the worker does a full resync (and refreshes OAuth tokens) periodically.
    async fn idle_inbox(&self, account_id: &str) -> Result<()> {
        let account = self.store.account(account_id).await?;
        let Some(inbox) = self.store.folder_by_role(account_id, FolderRole::Inbox).await? else {
            tokio::time::sleep(Duration::from_secs(120)).await;
            return Ok(());
        };
        let mut session = self.session(&account).await?;
        if !imap::supports_idle(&mut session).await? {
            session.logout().await.ok();
            tokio::time::sleep(Duration::from_secs(120)).await;
            return Ok(());
        }
        let started = tokio::time::Instant::now();
        loop {
            session.select(&inbox.path).await?;
            let remaining = IDLE_TIMEOUT.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                session.logout().await.ok();
                return Ok(());
            }
            let mut idle = session.idle();
            idle.init().await?;
            tracing::debug!(account = %account_id, "IDLE started");
            let (wait, _stop) = idle.wait_with_timeout(remaining.min(IDLE_REFRESH));
            let res = wait.await?;
            tracing::debug!(account = %account_id, "IDLE woke up: {res:?}");
            session = idle.done().await?;
            match res {
                async_imap::extensions::idle::IdleResponse::NewData(_) => {
                    let lock = self.sync_lock(account_id);
                    let _g = lock.lock().await;
                    let new = self.sync_folder(&mut session, &account, &inbox).await?;
                    drop(_g);
                    self.notify_new(&account, &inbox, new).await;
                    self.mailbox_changed(account_id).await;
                }
                // Refresh time: the next turn selects the inbox again (a cheap liveness check)
                // and re-issues IDLE, or returns once IDLE_TIMEOUT is up.
                async_imap::extensions::idle::IdleResponse::Timeout => {}
                async_imap::extensions::idle::IdleResponse::ManualInterrupt => {
                    session.logout().await.ok();
                    return Ok(());
                }
            }
        }
    }

    async fn notify_new(&self, account: &Account, folder: &Folder, new: Vec<MessageSummary>) {
        if new.is_empty() || folder.role != FolderRole::Inbox {
            return;
        }
        self.emit(Event::NewMail(NewMailEvent { account_id: account.id.clone(), folder_id: folder.id, messages: new }));
    }

    /// Full sync of an account: folder list plus the special folders' contents.
    pub async fn sync_account(&self, account_id: &str) -> Result<()> {
        let lock = self.sync_lock(account_id);
        let _g = lock.lock().await;
        self.emit(Event::SyncStatus { account_id: account_id.into(), syncing: true, error: None });
        // Reports the end of the sync even if it panics or is cancelled, so the UI's spinner stops.
        let mut status = SyncDone { sink: &*self.sink, account_id, error: Some("sincronización interrumpida".into()) };
        let result = self.sync_account_inner(account_id).await;
        status.error = result.as_ref().err().map(|e| e.to_string());
        drop(status);
        drop(_g);
        self.mailbox_changed(account_id).await;
        result
    }

    async fn sync_account_inner(&self, account_id: &str) -> Result<()> {
        let account = self.store.account(account_id).await?;
        let mut session = self.session(&account).await?;

        let remote = imap::list_folders(&mut session).await?;
        self.store.begin_folder_refresh(account_id).await?;
        for f in &remote {
            self.store.upsert_folder(account_id, &f.path, &f.name, f.role).await?;
        }
        self.store.finish_folder_refresh(account_id).await?;

        let folders = self.store.folders(account_id).await?;
        for role in [FolderRole::Inbox, FolderRole::Sent, FolderRole::Drafts, FolderRole::Archive, FolderRole::Junk, FolderRole::Trash] {
            if let Some(folder) = folders.iter().find(|f| f.role == role) {
                let new = self.sync_folder(&mut session, &account, folder).await?;
                self.notify_new(&account, folder, new).await;
            }
        }
        session.logout().await.ok();
        Ok(())
    }

    /// Syncs a non-special folder when the user opens it.
    pub async fn sync_folder_by_id(&self, folder_id: i64) -> Result<()> {
        let folder = self.store.folder(folder_id).await?;
        let account = self.store.account(&folder.account_id).await?;
        let lock = self.sync_lock(&account.id);
        let _g = lock.lock().await;
        let mut session = self.session(&account).await?;
        self.sync_folder(&mut session, &account, &folder).await?;
        session.logout().await.ok();
        drop(_g);
        self.mailbox_changed(&account.id).await;
        Ok(())
    }

    /// Incremental sync of one folder. Returns new unread messages (empty on first sync, so
    /// adding an account doesn't fire hundreds of notifications).
    async fn sync_folder(&self, session: &mut Session, account: &Account, folder: &Folder) -> Result<Vec<MessageSummary>> {
        let mbox = session.select(&folder.path).await?;
        if let Some(v) = mbox.uid_validity {
            self.store.set_uid_validity(folder.id, v).await?;
        }
        let state = self.store.folder_sync_state(folder.id).await?;
        let first_sync = state.last_uid.is_none();

        // 1. Flags and deletions for what we already have.
        if let (Some(min), Some(max)) = (state.min_uid, state.max_uid) {
            let remote: HashMap<u32, Flags> = if mbox.exists == 0 {
                HashMap::new()
            } else {
                imap::fetch_flags(session, &format!("{min}:{max}")).await?.into_iter().collect()
            };
            for (id, uid) in self.store.uids_in_range(folder.id, min, max).await? {
                match remote.get(&uid) {
                    Some(f) => self.store.set_flags(id, *f).await?,
                    None => self.store.delete_message(id).await?,
                }
            }
        }

        // 2. New messages.
        let headers = if mbox.exists == 0 {
            vec![]
        } else {
            match state.last_uid {
                None => {
                    let start = mbox.exists.saturating_sub(INITIAL_WINDOW - 1).max(1);
                    imap::fetch_headers_seq(session, &format!("{start}:*")).await?
                }
                Some(last) => imap::fetch_headers_uid(session, &format!("{}:*", last + 1))
                    .await?
                    .into_iter()
                    .filter(|h| h.uid > last) // "N:*" always returns the last message
                    .collect(),
            }
        };
        let mut new_unseen = Vec::new();
        for h in &headers {
            let parsed = parse_headers(&h.header);
            let (id, is_new) = self
                .store
                .upsert_header(&account.id, folder.id, h.meta(), &parsed)
                .await?;
            if is_new && !h.flags.seen && !first_sync {
                new_unseen.push(id);
            }
        }
        let highest = headers.iter().map(|h| h.uid).max().unwrap_or(0);
        let next = mbox.uid_next.map(|n| n.saturating_sub(1)).unwrap_or(0);
        self.store.set_last_uid(folder.id, highest.max(next)).await?;

        // 3. Prefetch recent bodies for previews.
        self.prefetch_bodies(session, folder.id).await?;

        let mut out = Vec::new();
        for id in new_unseen {
            out.push(self.store.message(id).await?);
        }
        Ok(out)
    }

    async fn prefetch_bodies(&self, session: &mut Session, folder_id: i64) -> Result<()> {
        let missing = self.store.missing_bodies(folder_id, PREFETCH_BODIES, PREFETCH_MAX_SIZE).await?;
        if missing.is_empty() {
            return Ok(());
        }
        let by_uid: HashMap<u32, i64> = missing.iter().map(|(id, uid)| (*uid, *id)).collect();
        let uids: Vec<u32> = by_uid.keys().copied().collect();
        for (uid, raw) in imap::fetch_raw(session, &uids).await? {
            if let Some(id) = by_uid.get(&uid) {
                self.save_body(*id, &raw).await?;
            }
        }
        Ok(())
    }

    async fn save_body(&self, id: i64, raw: &[u8]) -> Result<()> {
        match parse_full(raw, false) {
            Ok(p) => {
                let body = p.text.clone().unwrap_or_else(|| p.chat_text.clone());
                self.store.store_body(id, raw, &p.preview, &body, p.headers.has_attachments).await
            }
            Err(_) => self.store.store_body(id, raw, "", "", false).await,
        }
    }

    /// Loads older messages beyond the initial window ("load more" at the end of the list).
    pub async fn load_older(&self, folder_id: i64, count: usize) -> Result<usize> {
        let folder = self.store.folder(folder_id).await?;
        let account = self.store.account(&folder.account_id).await?;
        let state = self.store.folder_sync_state(folder_id).await?;
        let Some(min) = state.min_uid.filter(|m| *m > 1) else { return Ok(0) };
        let lock = self.sync_lock(&account.id);
        let _g = lock.lock().await;
        let mut session = self.session(&account).await?;
        session.select(&folder.path).await?;
        let mut uids: Vec<u32> = session.uid_search(format!("UID 1:{}", min - 1)).await?.into_iter().filter(|u| *u < min).collect();
        uids.sort_unstable();
        let take: Vec<u32> = uids.into_iter().rev().take(count).collect();
        if take.is_empty() {
            return Ok(0);
        }
        let headers = imap::fetch_headers_uid(&mut session, &imap::uid_set(&take)).await?;
        for h in &headers {
            let parsed = parse_headers(&h.header);
            self.store.upsert_header(&account.id, folder.id, h.meta(), &parsed).await?;
        }
        session.logout().await.ok();
        drop(_g);
        self.mailbox_changed(&account.id).await;
        Ok(headers.len())
    }

    // ---------------------------------------------------------------- reading

    async fn allow_remote(&self, from: Option<&EmailAddress>, force: bool) -> Result<bool> {
        if force || self.store.settings().await?.load_remote_images {
            return Ok(true);
        }
        match from {
            Some(f) => self.store.is_trusted(&f.email).await,
            None => Ok(false),
        }
    }

    /// Downloads bodies that aren't cached yet, one connection for all of them.
    async fn ensure_bodies(&self, ids: &[i64]) -> Result<()> {
        let mut missing: BTreeMap<(String, String), Vec<(i64, u32)>> = BTreeMap::new();
        for id in ids {
            if self.store.raw(*id).await?.is_none() {
                let loc = self.store.location(*id).await?;
                missing.entry((loc.account_id, loc.folder_path)).or_default().push((loc.id, loc.uid));
            }
        }
        let mut by_account: MissingBodies = BTreeMap::new();
        for ((acc, path), v) in missing {
            by_account.entry(acc).or_default().push((path, v));
        }
        for (account_id, folders) in by_account {
            let account = self.store.account(&account_id).await?;
            let mut session = self.session(&account).await?;
            for (path, msgs) in folders {
                session.select(&path).await?;
                let by_uid: HashMap<u32, i64> = msgs.iter().map(|(id, uid)| (*uid, *id)).collect();
                let uids: Vec<u32> = by_uid.keys().copied().collect();
                for (uid, raw) in imap::fetch_raw(&mut session, &uids).await? {
                    if let Some(id) = by_uid.get(&uid) {
                        self.save_body(*id, &raw).await?;
                    }
                }
            }
            session.logout().await.ok();
        }
        Ok(())
    }

    async fn body_of(&self, id: i64, force_remote: bool) -> Result<MessageBody> {
        let summary = self.store.message(id).await?;
        let raw = self
            .store
            .raw(id)
            .await?
            .ok_or_else(|| Error::NotFound("El mensaje ya no existe en el servidor".into()))?;
        let allow = self.allow_remote(summary.from.as_ref(), force_remote).await?;
        let p = parse_full(&raw, allow)?;
        Ok(MessageBody {
            reply_to: p.headers.reply_to,
            in_reply_to: p.headers.in_reply_to,
            references: p.headers.references,
            html: p.html,
            text: p.text,
            remote_content_blocked: p.remote_blocked,
            attachments: p.attachments,
            chat_text: p.chat_text,
            chat_html: p.chat_html,
            summary,
        })
    }

    pub async fn message(&self, id: i64, force_remote: bool) -> Result<MessageBody> {
        self.ensure_bodies(&[id]).await?;
        self.body_of(id, force_remote).await
    }

    /// Every message of a conversation, oldest first (classic and chat views).
    pub async fn thread(&self, account_id: &str, thread_id: &str, force_remote: bool) -> Result<Vec<MessageBody>> {
        let summaries = self.store.thread_summaries(account_id, thread_id).await?;
        let ids: Vec<i64> = summaries.iter().map(|s| s.id).collect();
        self.ensure_bodies(&ids).await?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            out.push(self.body_of(id, force_remote).await?);
        }
        Ok(out)
    }

    /// Name, MIME type and contents of an attachment (downloading the body if needed).
    pub async fn attachment_bytes(&self, message_id: i64, index: usize) -> Result<(String, String, Vec<u8>)> {
        self.ensure_bodies(&[message_id]).await?;
        let raw = self.store.raw(message_id).await?.ok_or_else(|| Error::NotFound("mensaje".into()))?;
        mime::attachment(&raw, index)
    }

    pub async fn save_attachment(&self, message_id: i64, index: usize, dest: &Path) -> Result<()> {
        let (_, _, bytes) = self.attachment_bytes(message_id, index).await?;
        tokio::fs::write(dest, bytes).await?;
        Ok(())
    }

    pub async fn attachment_name(&self, message_id: i64, index: usize) -> Result<String> {
        self.ensure_bodies(&[message_id]).await?;
        let raw = self.store.raw(message_id).await?.ok_or_else(|| Error::NotFound("mensaje".into()))?;
        Ok(mime::attachment(&raw, index)?.0)
    }

    // ---------------------------------------------------------------- actions

    /// Groups row ids by (account, folder path) for batched IMAP commands.
    async fn group(&self, ids: &[i64]) -> Result<BTreeMap<String, ByFolder>> {
        let mut out: BTreeMap<String, ByFolder> = BTreeMap::new();
        for id in ids {
            let loc = self.store.location(*id).await?;
            out.entry(loc.account_id)
                .or_default()
                .entry((loc.folder_id, loc.folder_path, loc.folder_role))
                .or_default()
                .push((loc.id, loc.uid));
        }
        Ok(out)
    }

    async fn remote_flag(&self, ids: &[i64], flag: &str, add: bool) -> Result<()> {
        for (account_id, folders) in self.group(ids).await? {
            let account = self.store.account(&account_id).await?;
            let mut session = self.session(&account).await?;
            for ((_, path, _), msgs) in folders {
                session.select(&path).await?;
                let uids: Vec<u32> = msgs.iter().map(|(_, u)| *u).collect();
                imap::store_flag(&mut session, &uids, flag, add).await?;
            }
            session.logout().await.ok();
            self.mailbox_changed(&account_id).await;
        }
        Ok(())
    }

    pub async fn set_seen(&self, ids: &[i64], seen: bool) -> Result<()> {
        for id in ids {
            self.store.set_seen(*id, seen).await?;
        }
        self.remote_flag(ids, "\\Seen", seen).await
    }

    pub async fn set_flagged(&self, ids: &[i64], flagged: bool) -> Result<()> {
        for id in ids {
            self.store.set_flagged(*id, flagged).await?;
        }
        self.remote_flag(ids, "\\Flagged", flagged).await
    }

    /// Row ids of a conversation limited to the given folders (e.g. archive a thread from the
    /// inbox without touching the user's own replies in Sent).
    pub async fn thread_rows(&self, account_id: &str, thread_id: &str, folder_ids: &[i64]) -> Result<Vec<i64>> {
        let mut out = Vec::new();
        for id in self.store.thread_row_ids(account_id, thread_id).await? {
            if folder_ids.is_empty() || folder_ids.contains(&self.store.location(id).await?.folder_id) {
                out.push(id);
            }
        }
        Ok(out)
    }

    async fn target_folder(&self, account: &Account, target: MoveTarget) -> Result<Option<Folder>> {
        let role = match target {
            MoveTarget::Inbox => FolderRole::Inbox,
            MoveTarget::Archive => FolderRole::Archive,
            MoveTarget::Junk => FolderRole::Junk,
            MoveTarget::Trash => FolderRole::Trash,
        };
        let mut f = self.store.folder_by_role(&account.id, role).await?;
        // Gmail has no Archive folder: archiving means leaving only the "All Mail" label.
        if f.is_none() && role == FolderRole::Archive {
            f = self.store.folder_by_role(&account.id, FolderRole::All).await?;
        }
        Ok(f)
    }

    pub async fn move_messages(&self, ids: &[i64], target: MoveTarget) -> Result<()> {
        for (account_id, folders) in self.group(ids).await? {
            let account = self.store.account(&account_id).await?;
            let dest = self.target_folder(&account, target).await?;
            let mut session = self.session(&account).await?;
            for ((folder_id, path, _role), msgs) in folders {
                let uids: Vec<u32> = msgs.iter().map(|(_, u)| *u).collect();
                session.select(&path).await?;
                match (&dest, target) {
                    (Some(d), _) if d.id == folder_id => {
                        // Deleting from the trash (or archiving from the archive): permanent.
                        if matches!(target, MoveTarget::Trash) {
                            imap::expunge(&mut session, &uids).await?;
                        } else {
                            continue;
                        }
                    }
                    (Some(d), _) => imap::move_to(&mut session, &uids, &d.path).await?,
                    (None, MoveTarget::Trash) => imap::expunge(&mut session, &uids).await?,
                    (None, _) => {
                        return Err(Error::NotFound(format!("La cuenta {} no tiene esa carpeta", account.email)));
                    }
                }
                for (id, _) in msgs {
                    self.store.delete_message(id).await?;
                }
            }
            if let Some(d) = dest {
                // Bring the moved copies into the destination folder right away.
                let _ = self.sync_folder(&mut session, &account, &d).await;
            }
            session.logout().await.ok();
            self.mailbox_changed(&account_id).await;
        }
        Ok(())
    }

    // ---------------------------------------------------------------- composing

    async fn forwarded(&self, out: &OutgoingMessage) -> Result<Vec<mime::MemoryAttachment>> {
        let Some(id) = out.forward_attachments_of else { return Ok(vec![]) };
        self.ensure_bodies(&[id]).await?;
        match self.store.raw(id).await? {
            Some(raw) => mime::forwardable_attachments(&raw),
            None => Ok(vec![]),
        }
    }

    pub async fn send(&self, out: &OutgoingMessage) -> Result<String> {
        let account = self.store.account(&out.account_id).await?;
        let built = mime::build_outgoing(&account, out, self.forwarded(out).await?, true)?;
        let raw = built.message.formatted();
        let cred = self.credential(&account).await?;
        crate::smtp::send(&account.smtp, &smtp_credential(&account, &cred), built.message)
            .await
            .map_err(|e| send_as_error(&account, e))?;

        // Gmail and Microsoft 365 store a copy in Sent automatically; generic servers don't.
        // Exchange puts mail sent *as* a shared mailbox in the sender's own Sent Items, so the
        // shared mailbox gets its copy here (a duplicate collapses by Message-ID if the admin
        // also enabled MessageCopyForSentAsEnabled).
        let mut session = imap::connect(&account.imap, &cred).await?;
        if account.provider == Provider::Imap || account.auth_user.is_some() {
            if let Some(sent) = self.store.folder_by_role(&account.id, FolderRole::Sent).await? {
                imap::append(&mut session, &sent.path, &raw, "(\\Seen)").await?;
            }
        }
        // Mark the original as answered.
        if let Some(parent) = &out.in_reply_to {
            let rows: Vec<i64> = self
                .store
                .search_by_message_id(&account.id, parent)
                .await?;
            for id in rows {
                let loc = self.store.location(id).await?;
                session.select(&loc.folder_path).await?;
                imap::store_flag(&mut session, &[loc.uid], "\\Answered", true).await.ok();
            }
        }
        if let Some(sent) = self.store.folder_by_role(&account.id, FolderRole::Sent).await? {
            let lock = self.sync_lock(&account.id);
            let _g = lock.lock().await;
            let _ = self.sync_folder(&mut session, &account, &sent).await;
        }
        session.logout().await.ok();
        self.mailbox_changed(&account.id).await;
        Ok(built.message_id)
    }

    /// Saves a draft in the server's Drafts folder, replacing the previous version.
    pub async fn save_draft(&self, out: &OutgoingMessage, replaces: Option<String>) -> Result<String> {
        let account = self.store.account(&out.account_id).await?;
        let drafts = self
            .store
            .folder_by_role(&account.id, FolderRole::Drafts)
            .await?
            .ok_or_else(|| Error::NotFound("La cuenta no tiene carpeta de borradores".into()))?;
        let built = mime::build_outgoing(&account, out, self.forwarded(out).await?, false)?;
        let raw = built.message.formatted();
        let mut session = self.session(&account).await?;
        imap::append(&mut session, &drafts.path, &raw, "(\\Seen \\Draft)").await?;
        if let Some(old) = replaces {
            session.select(&drafts.path).await?;
            if let Some(uid) = imap::find_by_message_id(&mut session, &old).await? {
                imap::expunge(&mut session, &[uid]).await?;
            }
        }
        let lock = self.sync_lock(&account.id);
        let _g = lock.lock().await;
        let _ = self.sync_folder(&mut session, &account, &drafts).await;
        session.logout().await.ok();
        drop(_g);
        self.mailbox_changed(&account.id).await;
        Ok(built.message_id)
    }

    pub async fn delete_draft(&self, account_id: &str, message_id: &str) -> Result<()> {
        let account = self.store.account(account_id).await?;
        let Some(drafts) = self.store.folder_by_role(account_id, FolderRole::Drafts).await? else { return Ok(()) };
        let mut session = self.session(&account).await?;
        session.select(&drafts.path).await?;
        if let Some(uid) = imap::find_by_message_id(&mut session, message_id).await? {
            imap::expunge(&mut session, &[uid]).await?;
        }
        for id in self.store.search_by_message_id(account_id, message_id).await? {
            if self.store.location(id).await?.folder_id == drafts.id {
                self.store.delete_message(id).await?;
            }
        }
        session.logout().await.ok();
        self.mailbox_changed(account_id).await;
        Ok(())
    }

    // ---------------------------------------------------------------- settings

    pub async fn trust_sender(&self, email: &str) -> Result<()> {
        self.store.trust_sender(email).await
    }
}

/// SMTP credential for an account: the same secret as IMAP, but shared mailboxes
/// authenticate as the delegate user.
/// Emits the "sync finished" status when dropped.
struct SyncDone<'a> {
    sink: &'a dyn EventSink,
    account_id: &'a str,
    error: Option<String>,
}

impl Drop for SyncDone<'_> {
    fn drop(&mut self) {
        self.sink.emit(Event::SyncStatus { account_id: self.account_id.into(), syncing: false, error: self.error.take() });
    }
}

fn smtp_credential(account: &Account, cred: &Credential) -> Credential {
    match cred {
        Credential::OAuth { access_token, .. } => {
            Credential::OAuth { user: account.smtp_user().to_string(), access_token: access_token.clone() }
        }
        Credential::Password { password, .. } => {
            Credential::Password { user: account.smtp_user().to_string(), password: password.clone() }
        }
    }
}

/// Turns Exchange's `5.2.252 SendAsDenied` into an actionable message for shared mailboxes.
fn send_as_error(account: &Account, e: Error) -> Error {
    match (&account.auth_user, &e) {
        (Some(user), Error::Smtp(inner)) if inner.to_string().contains("SendAsDenied") => Error::Auth(format!(
            "Tu usuario {user} no tiene permiso para enviar como {}. Pide al administrador el permiso \"Send As\" (enviar como) sobre ese buzón.",
            account.email
        )),
        _ => e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Security, ServerConfig};

    fn account(auth_user: Option<&str>) -> Account {
        let s = ServerConfig { host: "outlook.office365.com".into(), port: 993, security: Security::Tls };
        Account {
            id: "a".into(),
            provider: Provider::Outlook,
            email: "ventas@empresa.cl".into(),
            display_name: "ventas".into(),
            username: "ventas@empresa.cl".into(),
            imap: s.clone(),
            smtp: s,
            color: "#000".into(),
            auth_user: auth_user.map(Into::into),
        }
    }

    #[test]
    fn shared_mailbox_smtp_authenticates_as_delegate() {
        let imap = Credential::OAuth { user: "ventas@empresa.cl".into(), access_token: "t".into() };
        let shared = smtp_credential(&account(Some("juan@empresa.cl")), &imap);
        assert!(matches!(shared, Credential::OAuth { ref user, ref access_token } if user == "juan@empresa.cl" && access_token == "t"));
        let regular = smtp_credential(&account(None), &imap);
        assert!(matches!(regular, Credential::OAuth { ref user, .. } if user == "ventas@empresa.cl"));
    }
}
