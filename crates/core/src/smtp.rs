//! Sending through SMTP with mandatory TLS (implicit or STARTTLS).

use crate::error::Result;
use crate::imap::Credential;
use crate::model::{Security, ServerConfig};
use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::transport::smtp::client::{Certificate, Tls, TlsParameters};
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};

fn transport(cfg: &ServerConfig, cred: &Credential) -> Result<AsyncSmtpTransport<Tokio1Executor>> {
    let mut tls = TlsParameters::builder(cfg.host.clone());
    if let Some(pem) = crate::imap::extra_ca_pem() {
        tls = tls.add_root_certificate(Certificate::from_pem(&pem)?);
    }
    let tls = tls.build_rustls()?;
    let builder = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(cfg.host.as_str())
        .port(cfg.port)
        .timeout(Some(std::time::Duration::from_secs(60)))
        .tls(match cfg.security {
            Security::Tls => Tls::Wrapper(tls),
            // `Required`: abort if the server doesn't offer STARTTLS.
            Security::StartTls => Tls::Required(tls),
        });
    let builder = match cred {
        Credential::OAuth { user, access_token } => builder
            .credentials(Credentials::new(user.clone(), access_token.clone()))
            .authentication(vec![Mechanism::Xoauth2]),
        Credential::Password { user, password } => builder
            .credentials(Credentials::new(user.clone(), password.clone()))
            .authentication(vec![Mechanism::Plain, Mechanism::Login]),
    };
    Ok(builder.build())
}

pub async fn send(cfg: &ServerConfig, cred: &Credential, message: lettre::Message) -> Result<()> {
    transport(cfg, cred)?.send(message).await?;
    Ok(())
}

/// Checks that the server accepts the credentials (used by the account wizard).
pub async fn verify(cfg: &ServerConfig, cred: &Credential) -> Result<()> {
    transport(cfg, cred)?.test_connection().await?;
    Ok(())
}
