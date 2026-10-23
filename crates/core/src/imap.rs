//! IMAP client: TLS-only connections, XOAUTH2/LOGIN auth, folder discovery and message ops.

use crate::error::{Error, Result};
use crate::model::{FolderRole, Security, ServerConfig};
use crate::oauth::xoauth2_payload;
use crate::store::Flags;
use crate::utf7;
use async_imap::imap_proto::NameAttribute;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use async_imap::types::Flag;
use futures::TryStreamExt;
use std::sync::{Arc, LazyLock};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::{self, pki_types::ServerName};
use tokio_rustls::TlsConnector;

pub type Stream = TlsStream<TcpStream>;
pub type Session = async_imap::Session<Stream>;

#[derive(Clone)]
pub enum Credential {
    OAuth { user: String, access_token: String },
    Password { user: String, password: String },
}

impl std::fmt::Debug for Credential {
    // Never print secrets in logs.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Credential::OAuth { user, .. } => write!(f, "OAuth({user})"),
            Credential::Password { user, .. } => write!(f, "Password({user})"),
        }
    }
}

/// Extra trusted CA certificates (PEM) from `CORNAMUSA_EXTRA_CA`, for self-hosted servers
/// with a private CA. Certificate verification is never disabled.
pub fn extra_ca_pem() -> Option<Vec<u8>> {
    let path = std::env::var_os("CORNAMUSA_EXTRA_CA")?;
    match std::fs::read(&path) {
        Ok(pem) => Some(pem),
        Err(e) => {
            tracing::warn!("CORNAMUSA_EXTRA_CA {:?}: {e}", path);
            None
        }
    }
}

static TLS: LazyLock<Arc<rustls::ClientConfig>> = LazyLock::new(|| {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    // System roots too, so servers signed by a CA installed in the OS work.
    for cert in rustls_native_certs::load_native_certs().certs {
        let _ = roots.add(cert);
    }
    if let Some(pem) = extra_ca_pem() {
        for cert in rustls_pemfile::certs(&mut pem.as_slice()).flatten() {
            let _ = roots.add(cert);
        }
    }
    Arc::new(rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth())
});

pub fn tls_config() -> Arc<rustls::ClientConfig> {
    TLS.clone()
}

async fn tls_wrap(tcp: TcpStream, host: &str) -> Result<Stream> {
    let name = ServerName::try_from(host.to_string()).map_err(|e| Error::Tls(e.to_string()))?;
    TlsConnector::from(tls_config())
        .connect(name, tcp)
        .await
        .map_err(|e| Error::Tls(format!("{host}: {e}")))
}

/// STARTTLS upgrade done by hand before handing the stream to async-imap. If the server
/// doesn't accept STARTTLS the connection fails: there is no plaintext fallback.
async fn starttls(tcp: TcpStream, host: &str) -> Result<Stream> {
    let mut reader = BufReader::new(tcp);
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    if !line.starts_with("* OK") {
        return Err(Error::Tls(format!("Saludo IMAP inesperado: {}", line.trim())));
    }
    reader.get_mut().write_all(b"a0 STARTTLS\r\n").await?;
    loop {
        line.clear();
        if reader.read_line(&mut line).await? == 0 {
            return Err(Error::Tls("Conexión cerrada durante STARTTLS".into()));
        }
        if line.starts_with("a0 ") {
            if line.starts_with("a0 OK") {
                break;
            }
            return Err(Error::Tls(format!("El servidor rechazó STARTTLS: {}", line.trim())));
        }
    }
    tls_wrap(reader.into_inner(), host).await
}

/// Only ever answers the server's error challenge: the credential travels as SASL-IR.
struct XOAuth2Error;

impl async_imap::Authenticator for XOAuth2Error {
    type Response = &'static str;
    fn process(&mut self, _challenge: &[u8]) -> &'static str {
        // On failure the server sends a JSON error challenge; an empty reply ends the exchange.
        ""
    }
}

pub async fn connect(cfg: &ServerConfig, cred: &Credential) -> Result<Session> {
    let tcp = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        TcpStream::connect((cfg.host.as_str(), cfg.port)),
    )
    .await
    .map_err(|_| Error::Other(format!("Tiempo de espera agotado conectando a {}", cfg.host)))??;
    let _ = tcp.set_nodelay(true);

    let client = match cfg.security {
        Security::Tls => {
            let mut c = async_imap::Client::new(tls_wrap(tcp, &cfg.host).await?);
            c.read_response()
                .await
                .ok_or_else(|| Error::Other("El servidor cerró la conexión".into()))??;
            c
        }
        Security::StartTls => async_imap::Client::new(starttls(tcp, &cfg.host).await?),
    };

    let session = match cred {
        // XOAUTH2 is only used with Gmail and Microsoft, which both advertise SASL-IR (RFC 4959),
        // so the initial response goes on the AUTHENTICATE line itself.
        Credential::OAuth { user, access_token } => client
            .authenticate(
                format!("XOAUTH2 {}", STANDARD.encode(xoauth2_payload(user, access_token))),
                XOAuth2Error,
            )
            .await
            .map_err(|(e, _)| Error::Auth(format!("IMAP XOAUTH2: {e}")))?,
        Credential::Password { user, password } => client
            .login(user, password)
            .await
            .map_err(|(e, _)| Error::Auth(format!("IMAP: {e}")))?,
    };
    Ok(session)
}

#[derive(Debug, Clone)]
pub struct RemoteFolder {
    pub path: String,
    pub name: String,
    pub role: FolderRole,
}

/// Role from RFC 6154 SPECIAL-USE attributes, falling back to well-known names (several
/// languages) for servers that don't advertise them.
pub fn folder_role(path: &str, attrs: &[NameAttribute]) -> FolderRole {
    if path.eq_ignore_ascii_case("INBOX") {
        return FolderRole::Inbox;
    }
    for a in attrs {
        let role = match a {
            NameAttribute::Sent => FolderRole::Sent,
            NameAttribute::Drafts => FolderRole::Drafts,
            NameAttribute::Junk => FolderRole::Junk,
            NameAttribute::Trash => FolderRole::Trash,
            NameAttribute::Archive => FolderRole::Archive,
            NameAttribute::All => FolderRole::All,
            NameAttribute::Flagged => FolderRole::Flagged,
            _ => continue,
        };
        return role;
    }
    let leaf = utf7::decode(path).rsplit(['/', '.']).next().unwrap_or_default().to_lowercase();
    match leaf.as_str() {
        "sent" | "sent items" | "sent mail" | "sent messages" | "enviados" | "elementos enviados" | "enviado"
        | "correo enviado" => FolderRole::Sent,
        "drafts" | "draft" | "borradores" | "borrador" => FolderRole::Drafts,
        "junk" | "spam" | "junk email" | "junk e-mail" | "bulk mail" | "correo no deseado" | "no deseado" => {
            FolderRole::Junk
        }
        "trash" | "deleted" | "deleted items" | "deleted messages" | "papelera" | "elementos eliminados" => {
            FolderRole::Trash
        }
        "archive" | "archives" | "archivo" | "archivados" => FolderRole::Archive,
        _ => FolderRole::Other,
    }
}

fn display_name(path: &str, delimiter: Option<&str>, role: FolderRole) -> String {
    let localized = match role {
        FolderRole::Inbox => Some("Recibidos"),
        FolderRole::Sent => Some("Enviados"),
        FolderRole::Drafts => Some("Borradores"),
        FolderRole::Junk => Some("No deseado"),
        FolderRole::Trash => Some("Papelera"),
        FolderRole::Archive => Some("Archivo"),
        FolderRole::All => Some("Todos"),
        FolderRole::Flagged => Some("Destacados"),
        FolderRole::Other => None,
    };
    if let Some(name) = localized {
        return name.into();
    }
    let decoded = utf7::decode(path);
    match delimiter {
        // Keep the hierarchy readable: "Proyectos / Cliente A".
        Some(d) if !d.is_empty() => decoded
            .trim_start_matches("[Gmail]")
            .split(d)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" / "),
        _ => decoded,
    }
}

pub async fn list_folders(session: &mut Session) -> Result<Vec<RemoteFolder>> {
    let names: Vec<_> = session.list(Some(""), Some("*")).await?.try_collect().await?;
    let mut out: Vec<RemoteFolder> = Vec::new();
    for n in &names {
        if n.attributes().iter().any(|a| matches!(a, NameAttribute::NoSelect)) {
            continue;
        }
        let path = n.name().to_string();
        let mut role = folder_role(&path, n.attributes());
        // Only one folder per special role; extra matches stay as normal folders.
        if role != FolderRole::Other && out.iter().any(|f| f.role == role) {
            role = FolderRole::Other;
        }
        out.push(RemoteFolder { name: display_name(&path, n.delimiter(), role), path, role });
    }
    Ok(out)
}

pub struct FetchedHeader {
    pub uid: u32,
    pub flags: Flags,
    pub internal_date: Option<i64>,
    pub size: u32,
    pub header: Vec<u8>,
}

impl FetchedHeader {
    pub fn meta(&self) -> crate::store::HeaderMeta {
        crate::store::HeaderMeta { uid: self.uid, flags: self.flags, internal_date: self.internal_date, size: self.size }
    }
}

fn flags_of(f: &async_imap::types::Fetch) -> Flags {
    let mut out = Flags::default();
    for flag in f.flags() {
        match flag {
            Flag::Seen => out.seen = true,
            Flag::Flagged => out.flagged = true,
            Flag::Answered => out.answered = true,
            _ => {}
        }
    }
    out
}

const HEADER_QUERY: &str = "(UID FLAGS INTERNALDATE RFC822.SIZE BODY.PEEK[HEADER])";

fn to_header(f: &async_imap::types::Fetch) -> Option<FetchedHeader> {
    Some(FetchedHeader {
        uid: f.uid?,
        flags: flags_of(f),
        internal_date: f.internal_date().map(|d| d.timestamp()),
        size: f.size.unwrap_or(0),
        header: f.header().unwrap_or_default().to_vec(),
    })
}

/// Headers by sequence range (initial sync of the newest N messages).
pub async fn fetch_headers_seq(session: &mut Session, range: &str) -> Result<Vec<FetchedHeader>> {
    let fetches: Vec<_> = session.fetch(range, HEADER_QUERY).await?.try_collect().await?;
    Ok(fetches.iter().filter_map(to_header).collect())
}

pub async fn fetch_headers_uid(session: &mut Session, uid_range: &str) -> Result<Vec<FetchedHeader>> {
    let fetches: Vec<_> = session.uid_fetch(uid_range, HEADER_QUERY).await?.try_collect().await?;
    Ok(fetches.iter().filter_map(to_header).collect())
}

pub async fn fetch_flags(session: &mut Session, uid_range: &str) -> Result<Vec<(u32, Flags)>> {
    let fetches: Vec<_> = session.uid_fetch(uid_range, "(UID FLAGS)").await?.try_collect().await?;
    Ok(fetches.iter().filter_map(|f| Some((f.uid?, flags_of(f)))).collect())
}

/// Full raw messages for the given UIDs (without setting \Seen).
pub async fn fetch_raw(session: &mut Session, uids: &[u32]) -> Result<Vec<(u32, Vec<u8>)>> {
    if uids.is_empty() {
        return Ok(vec![]);
    }
    let set = uid_set(uids);
    let fetches: Vec<_> = session.uid_fetch(&set, "(UID BODY.PEEK[])").await?.try_collect().await?;
    Ok(fetches
        .iter()
        .filter_map(|f| Some((f.uid?, f.body()?.to_vec())))
        .collect())
}

pub fn uid_set(uids: &[u32]) -> String {
    let mut sorted = uids.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut parts = Vec::new();
    let mut i = 0;
    while i < sorted.len() {
        let start = sorted[i];
        let mut end = start;
        while i + 1 < sorted.len() && sorted[i + 1] == end + 1 {
            i += 1;
            end = sorted[i];
        }
        parts.push(if start == end { start.to_string() } else { format!("{start}:{end}") });
        i += 1;
    }
    parts.join(",")
}

pub async fn store_flag(session: &mut Session, uids: &[u32], flag: &str, add: bool) -> Result<()> {
    let op = if add { "+FLAGS.SILENT" } else { "-FLAGS.SILENT" };
    let _: Vec<_> = session.uid_store(uid_set(uids), format!("{op} ({flag})")).await?.try_collect().await?;
    Ok(())
}

fn quote(mailbox: &str) -> String {
    format!("\"{}\"", mailbox.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Moves messages; uses MOVE (RFC 6851) when available, otherwise COPY + \Deleted + EXPUNGE.
pub async fn move_to(session: &mut Session, uids: &[u32], dest: &str) -> Result<()> {
    let caps = session.capabilities().await?;
    let set = uid_set(uids);
    if caps.has_str("MOVE") {
        // uid_mv quotes the mailbox itself; uid_copy doesn't.
        session.uid_mv(&set, dest).await?;
    } else {
        session.uid_copy(&set, quote(dest)).await?;
        expunge(session, uids).await?;
    }
    Ok(())
}

/// Permanently deletes messages from the selected folder.
pub async fn expunge(session: &mut Session, uids: &[u32]) -> Result<()> {
    store_flag(session, uids, "\\Deleted", true).await?;
    let caps = session.capabilities().await?;
    if caps.has_str("UIDPLUS") {
        let _: Vec<_> = session.uid_expunge(uid_set(uids)).await?.try_collect().await?;
    } else {
        let _: Vec<_> = session.expunge().await?.try_collect().await?;
    }
    Ok(())
}

pub async fn append(session: &mut Session, folder: &str, raw: &[u8], flags: &str) -> Result<()> {
    session.append(folder, Some(flags), None, raw).await?;
    Ok(())
}

pub async fn find_by_message_id(session: &mut Session, message_id: &str) -> Result<Option<u32>> {
    let id = message_id.replace('"', "");
    let found = session.uid_search(format!("HEADER Message-ID \"<{id}>\"")).await?;
    Ok(found.into_iter().max())
}

pub async fn supports_idle(session: &mut Session) -> Result<bool> {
    Ok(session.capabilities().await?.has_str("IDLE"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uid_sets() {
        assert_eq!(uid_set(&[5, 1, 2, 3, 9, 10]), "1:3,5,9:10");
        assert_eq!(uid_set(&[7]), "7");
        assert_eq!(uid_set(&[4, 4]), "4");
    }

    #[test]
    fn roles() {
        assert_eq!(folder_role("INBOX", &[]), FolderRole::Inbox);
        assert_eq!(folder_role("[Gmail]/Spam", &[NameAttribute::Junk]), FolderRole::Junk);
        assert_eq!(folder_role("Elementos enviados", &[]), FolderRole::Sent);
        assert_eq!(folder_role("INBOX.Correo no deseado", &[]), FolderRole::Junk);
        assert_eq!(folder_role("Deleted Items", &[]), FolderRole::Trash);
        assert_eq!(folder_role("Proyectos", &[]), FolderRole::Other);
    }

    #[test]
    fn names() {
        assert_eq!(display_name("[Gmail]/Enviados", Some("/"), FolderRole::Sent), "Enviados");
        assert_eq!(display_name("Proyectos/Cliente A", Some("/"), FolderRole::Other), "Proyectos / Cliente A");
        assert_eq!(display_name("Facturaci&APM-n", Some("/"), FolderRole::Other), "Facturación");
    }
}
