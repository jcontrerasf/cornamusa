//! OAuth2 Authorization Code flow with PKCE and a loopback redirect (RFC 8252).
//!
//! The login always happens in the user's default browser, never in an embedded webview,
//! so the app never sees the user's password.

use crate::error::{Error, Result};
use crate::model::Provider;
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;
use rand::distributions::Alphanumeric;
use rand::Rng;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Debug, Clone)]
pub struct OAuthClient {
    pub client_id: String,
    /// Google "Desktop app" clients ship a non-confidential secret; Microsoft public clients don't.
    pub client_secret: Option<String>,
    pub auth_url: &'static str,
    pub token_url: &'static str,
    pub scopes: &'static str,
    /// Host used in the redirect URI (`127.0.0.1` for Google, `localhost` for Microsoft).
    pub redirect_host: &'static str,
}

fn env_or_build(runtime: &str, build: Option<&'static str>) -> Option<String> {
    std::env::var(runtime)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| build.map(str::to_string).filter(|v| !v.is_empty()))
}

/// Client IDs are read from the environment at runtime, falling back to values baked in at
/// build time (`CORNAMUSA_GOOGLE_CLIENT_ID=... cargo build`). They are never committed.
pub fn client_for(provider: Provider) -> Result<OAuthClient> {
    match provider {
        Provider::Gmail => Ok(OAuthClient {
            client_id: env_or_build("CORNAMUSA_GOOGLE_CLIENT_ID", option_env!("CORNAMUSA_GOOGLE_CLIENT_ID"))
                .ok_or_else(|| Error::Config("Falta CORNAMUSA_GOOGLE_CLIENT_ID (ver README).".into()))?,
            client_secret: env_or_build(
                "CORNAMUSA_GOOGLE_CLIENT_SECRET",
                option_env!("CORNAMUSA_GOOGLE_CLIENT_SECRET"),
            ),
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth",
            token_url: "https://oauth2.googleapis.com/token",
            scopes: "https://mail.google.com/ openid email profile",
            redirect_host: "127.0.0.1",
        }),
        Provider::Outlook => Ok(OAuthClient {
            client_id: env_or_build("CORNAMUSA_MS_CLIENT_ID", option_env!("CORNAMUSA_MS_CLIENT_ID"))
                .ok_or_else(|| Error::Config("Falta CORNAMUSA_MS_CLIENT_ID (ver README).".into()))?,
            client_secret: None,
            auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
            token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token",
            scopes: "https://outlook.office.com/IMAP.AccessAsUser.All https://outlook.office.com/SMTP.Send offline_access openid email profile",
            redirect_host: "localhost",
        }),
        Provider::Imap => Err(Error::Config("Las cuentas IMAP genéricas no usan OAuth".into())),
    }
}

#[derive(Debug, Clone)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Unix timestamp when the access token expires.
    pub expires_at: i64,
    pub id_token: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Identity {
    pub email: Option<String>,
    pub name: Option<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    id_token: Option<String>,
}

#[derive(Deserialize)]
struct TokenError {
    error: String,
    error_description: Option<String>,
}

fn random_string(len: usize) -> String {
    rand::thread_rng().sample_iter(&Alphanumeric).take(len).map(char::from).collect()
}

pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Extracts identity claims from an id_token. The token was received directly from the
/// provider's token endpoint over TLS, so its signature doesn't need to be verified here.
pub fn identity_from_id_token(id_token: &str) -> Identity {
    let payload = id_token.split('.').nth(1).unwrap_or_default();
    let decoded = URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .or_else(|_| URL_SAFE.decode(payload))
        .unwrap_or_default();
    let v: serde_json::Value = serde_json::from_slice(&decoded).unwrap_or_default();
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_string);
    Identity { email: s("email").or_else(|| s("preferred_username")), name: s("name") }
}

fn parse_query(path: &str) -> Vec<(String, String)> {
    let query = path.split_once('?').map(|(_, q)| q).unwrap_or_default();
    url::form_urlencoded::parse(query.as_bytes()).into_owned().collect()
}

const DONE_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Cornamusa</title>\
<body style=\"font-family:system-ui;display:grid;place-items:center;height:100vh;margin:0\">\
<div style=\"text-align:center\"><h2>✅ Cuenta conectada</h2><p>Ya puedes cerrar esta pestaña y volver a Cornamusa.</p></div>";

const ERROR_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Cornamusa</title>\
<body style=\"font-family:system-ui;display:grid;place-items:center;height:100vh;margin:0\">\
<div style=\"text-align:center\"><h2>No se pudo conectar la cuenta</h2><p>Vuelve a Cornamusa e inténtalo de nuevo.</p></div>";

/// Waits on the loopback listener for the browser redirect carrying the authorization code.
async fn wait_for_code(listener: TcpListener, expected_state: &str) -> Result<String> {
    loop {
        let (mut sock, _) = listener.accept().await?;
        let mut buf = vec![0u8; 8192];
        let n = sock.read(&mut buf).await?;
        let req = String::from_utf8_lossy(&buf[..n]);
        let path = req.lines().next().and_then(|l| l.split_whitespace().nth(1)).unwrap_or("/");
        let params = parse_query(path);
        let get = |k: &str| params.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());

        if get("code").is_none() && get("error").is_none() {
            // favicon.ico or a stray request: ignore it.
            let _ = sock.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
            continue;
        }

        let result = match (get("state"), get("code"), get("error")) {
            (Some(state), Some(code), None) if state == expected_state => Ok(code),
            (_, _, Some(err)) => Err(Error::Auth(format!(
                "El proveedor rechazó el acceso: {err} {}",
                get("error_description").unwrap_or_default()
            ))),
            _ => Err(Error::Auth("Respuesta OAuth inválida (state no coincide)".into())),
        };
        let page = if result.is_ok() { DONE_PAGE } else { ERROR_PAGE };
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
            page.len()
        );
        let _ = sock.write_all(resp.as_bytes()).await;
        let _ = sock.shutdown().await;
        return result;
    }
}

async fn token_request(client: &OAuthClient, mut form: Vec<(&str, String)>) -> Result<TokenSet> {
    form.push(("client_id", client.client_id.clone()));
    if let Some(secret) = &client.client_secret {
        form.push(("client_secret", secret.clone()));
    }
    let resp = reqwest::Client::new()
        .post(client.token_url)
        .form(&form)
        .timeout(Duration::from_secs(30))
        .send()
        .await?;
    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        let msg = serde_json::from_str::<TokenError>(&text)
            .map(|e| format!("{}: {}", e.error, e.error_description.unwrap_or_default()))
            .unwrap_or(text);
        return Err(Error::Auth(msg));
    }
    let t: TokenResponse = resp.json().await?;
    Ok(TokenSet {
        access_token: t.access_token,
        refresh_token: t.refresh_token,
        expires_at: chrono::Utc::now().timestamp() + t.expires_in.unwrap_or(3600),
        id_token: t.id_token,
    })
}

/// Runs the interactive login. `open_browser` receives the authorization URL and must open
/// it in the system browser.
pub async fn authorize(
    provider: Provider,
    login_hint: Option<&str>,
    open_browser: impl FnOnce(&str) -> Result<()>,
) -> Result<(TokenSet, Identity)> {
    let client = client_for(provider)?;
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://{}:{port}", client.redirect_host);
    let verifier = random_string(64);
    let state = random_string(32);

    let mut url = url::Url::parse(client.auth_url).expect("static url");
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("client_id", &client.client_id)
            .append_pair("response_type", "code")
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("scope", client.scopes)
            .append_pair("state", &state)
            .append_pair("code_challenge", &pkce_challenge(&verifier))
            .append_pair("code_challenge_method", "S256");
        if provider == Provider::Gmail {
            // Needed to receive a refresh token every time.
            q.append_pair("access_type", "offline").append_pair("prompt", "consent");
        } else {
            q.append_pair("prompt", "select_account");
        }
        if let Some(hint) = login_hint.filter(|h| !h.is_empty()) {
            q.append_pair("login_hint", hint);
        }
    }
    open_browser(url.as_str())?;

    let code = tokio::time::timeout(Duration::from_secs(300), wait_for_code(listener, &state))
        .await
        .map_err(|_| Error::Auth("Tiempo de espera agotado para el inicio de sesión".into()))??;

    let tokens = token_request(
        &client,
        vec![
            ("grant_type", "authorization_code".into()),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("code_verifier", verifier),
        ],
    )
    .await?;
    let identity = tokens.id_token.as_deref().map(identity_from_id_token).unwrap_or_default();
    Ok((tokens, identity))
}

pub async fn refresh(provider: Provider, refresh_token: &str) -> Result<TokenSet> {
    let client = client_for(provider)?;
    let mut form = vec![
        ("grant_type", "refresh_token".to_string()),
        ("refresh_token", refresh_token.to_string()),
    ];
    if provider == Provider::Outlook {
        form.push(("scope", client.scopes.to_string()));
    }
    token_request(&client, form).await
}

/// SASL XOAUTH2 initial response (before base64), shared by IMAP and SMTP.
pub fn xoauth2_payload(user: &str, access_token: &str) -> String {
    format!("user={user}\x01auth=Bearer {access_token}\x01\x01")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_rfc7636_vector() {
        // Appendix B of RFC 7636.
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn id_token_claims() {
        let payload = URL_SAFE_NO_PAD.encode(br#"{"email":"ana@gmail.com","name":"Ana"}"#);
        let id = identity_from_id_token(&format!("x.{payload}.y"));
        assert_eq!(id.email.as_deref(), Some("ana@gmail.com"));
        assert_eq!(id.name.as_deref(), Some("Ana"));
        let ms = URL_SAFE_NO_PAD.encode(br#"{"preferred_username":"bo@outlook.com"}"#);
        assert_eq!(identity_from_id_token(&format!("x.{ms}.y")).email.as_deref(), Some("bo@outlook.com"));
    }

    #[tokio::test]
    async fn loopback_validates_state() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move { wait_for_code(listener, "abc").await });
        let send = |path: String| async move {
            let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
            s.write_all(format!("GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes()).await.unwrap();
            let mut out = String::new();
            let _ = s.read_to_string(&mut out).await;
            out
        };
        send("/favicon.ico".into()).await;
        let page = send("/?code=XYZ&state=abc".into()).await;
        assert!(page.contains("Cuenta conectada"));
        assert_eq!(task.await.unwrap().unwrap(), "XYZ");
    }
}
