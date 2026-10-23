//! Server presets and autodetection for account setup.

use crate::model::{Provider, Security, ServerConfig};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedConfig {
    pub provider: Provider,
    pub imap: ServerConfig,
    pub smtp: ServerConfig,
    /// Hint for providers that require an app password instead of the normal one.
    pub app_password_hint: Option<String>,
}

fn server(host: &str, port: u16, security: Security) -> ServerConfig {
    ServerConfig { host: host.into(), port, security }
}

pub fn gmail() -> DetectedConfig {
    DetectedConfig {
        provider: Provider::Gmail,
        imap: server("imap.gmail.com", 993, Security::Tls),
        smtp: server("smtp.gmail.com", 465, Security::Tls),
        app_password_hint: None,
    }
}

pub fn outlook() -> DetectedConfig {
    DetectedConfig {
        provider: Provider::Outlook,
        imap: server("outlook.office365.com", 993, Security::Tls),
        smtp: server("smtp.office365.com", 587, Security::StartTls),
        app_password_hint: None,
    }
}

pub fn preset(provider: Provider) -> Option<DetectedConfig> {
    match provider {
        Provider::Gmail => Some(gmail()),
        Provider::Outlook => Some(outlook()),
        Provider::Imap => None,
    }
}

fn domain_of(email: &str) -> Option<String> {
    email.rsplit_once('@').map(|(_, d)| d.trim().to_ascii_lowercase()).filter(|d| !d.is_empty())
}

/// Known providers that don't need a network lookup.
fn known(domain: &str) -> Option<DetectedConfig> {
    let generic = |imap: &str, smtp: &str, smtp_port: u16, hint: Option<&str>| DetectedConfig {
        provider: Provider::Imap,
        imap: server(imap, 993, Security::Tls),
        smtp: server(smtp, smtp_port, if smtp_port == 587 { Security::StartTls } else { Security::Tls }),
        app_password_hint: hint.map(Into::into),
    };
    match domain {
        "gmail.com" | "googlemail.com" => Some(gmail()),
        "outlook.com" | "hotmail.com" | "live.com" | "msn.com" | "outlook.es" | "hotmail.es"
        | "live.cl" | "hotmail.cl" | "live.com.mx" | "outlook.cl" => Some(outlook()),
        d if d.starts_with("hotmail.") || d.starts_with("outlook.") || d.starts_with("live.") => {
            Some(outlook())
        }
        "yahoo.com" | "yahoo.es" | "ymail.com" | "rocketmail.com" => Some(generic(
            "imap.mail.yahoo.com",
            "smtp.mail.yahoo.com",
            465,
            Some("Yahoo requiere una contraseña de aplicación (Seguridad de la cuenta → Generar contraseña de app)."),
        )),
        "icloud.com" | "me.com" | "mac.com" => Some(generic(
            "imap.mail.me.com",
            "smtp.mail.me.com",
            587,
            Some("iCloud requiere una contraseña específica de app (appleid.apple.com)."),
        )),
        "fastmail.com" | "fastmail.fm" => Some(generic(
            "imap.fastmail.com",
            "smtp.fastmail.com",
            465,
            Some("Fastmail requiere una contraseña de aplicación."),
        )),
        "gmx.com" | "gmx.net" | "gmx.de" | "gmx.es" => {
            Some(generic("imap.gmx.com", "mail.gmx.com", 587, None))
        }
        "zoho.com" | "zohomail.com" => Some(generic("imap.zoho.com", "smtp.zoho.com", 465, None)),
        "proton.me" | "protonmail.com" => Some(DetectedConfig {
            provider: Provider::Imap,
            imap: server("127.0.0.1", 1143, Security::StartTls),
            smtp: server("127.0.0.1", 1025, Security::StartTls),
            app_password_hint: Some("Proton requiere Proton Mail Bridge ejecutándose localmente.".into()),
        }),
        _ => None,
    }
}

/// Very small extraction of `<incomingServer type="imap">` / `<outgoingServer type="smtp">`
/// from Mozilla's ISPDB autoconfig XML.
fn parse_ispdb(xml: &str) -> Option<(ServerConfig, ServerConfig)> {
    fn block<'a>(xml: &'a str, open: &str, close: &str) -> Option<&'a str> {
        let start = xml.find(open)?;
        let rest = &xml[start..];
        let end = rest.find(close)?;
        Some(&rest[..end])
    }
    fn tag(block: &str, name: &str) -> Option<String> {
        let open = format!("<{name}>");
        let start = block.find(&open)? + open.len();
        let end = block[start..].find(&format!("</{name}>"))? + start;
        Some(block[start..end].trim().to_string())
    }
    fn to_server(b: &str) -> Option<ServerConfig> {
        let host = tag(b, "hostname")?;
        let port = tag(b, "port")?.parse().ok()?;
        let security = match tag(b, "socketType")?.as_str() {
            "SSL" => Security::Tls,
            "STARTTLS" => Security::StartTls,
            _ => return None, // never accept plaintext servers
        };
        Some(ServerConfig { host, port, security })
    }
    let imap = to_server(block(xml, "<incomingServer type=\"imap\">", "</incomingServer>")?)?;
    let smtp = to_server(block(xml, "<outgoingServer type=\"smtp\">", "</outgoingServer>")?)?;
    Some((imap, smtp))
}

/// Detect server settings for an email address: known list, then Mozilla ISPDB,
/// then a conventional `imap.<domain>` / `smtp.<domain>` guess.
pub async fn detect(email: &str) -> DetectedConfig {
    let Some(domain) = domain_of(email) else {
        return guess("example.com");
    };
    if let Some(cfg) = known(&domain) {
        return cfg;
    }
    let url = format!("https://autoconfig.thunderbird.net/v1.1/{domain}");
    if let Ok(resp) = reqwest::Client::new()
        .get(&url)
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await
    {
        if resp.status().is_success() {
            if let Ok(xml) = resp.text().await {
                if let Some((imap, smtp)) = parse_ispdb(&xml) {
                    let provider = if imap.host.ends_with("gmail.com") {
                        Provider::Gmail
                    } else if imap.host.ends_with("office365.com") || imap.host.ends_with("outlook.com") {
                        Provider::Outlook
                    } else {
                        Provider::Imap
                    };
                    return DetectedConfig { provider, imap, smtp, app_password_hint: None };
                }
            }
        }
    }
    guess(&domain)
}

fn guess(domain: &str) -> DetectedConfig {
    DetectedConfig {
        provider: Provider::Imap,
        imap: server(&format!("imap.{domain}"), 993, Security::Tls),
        smtp: server(&format!("smtp.{domain}"), 465, Security::Tls),
        app_password_hint: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_domains() {
        assert_eq!(known("gmail.com").unwrap().provider, Provider::Gmail);
        assert_eq!(known("hotmail.es").unwrap().provider, Provider::Outlook);
        assert_eq!(known("hotmail.co.uk").unwrap().provider, Provider::Outlook);
        let y = known("yahoo.com").unwrap();
        assert_eq!(y.provider, Provider::Imap);
        assert!(y.app_password_hint.is_some());
        assert!(known("midominio.cl").is_none());
    }

    #[test]
    fn ispdb_rejects_plaintext() {
        let xml = r#"<clientConfig><emailProvider>
            <incomingServer type="imap"><hostname>imap.x.org</hostname><port>143</port><socketType>plain</socketType></incomingServer>
            <outgoingServer type="smtp"><hostname>smtp.x.org</hostname><port>587</port><socketType>STARTTLS</socketType></outgoingServer>
        </emailProvider></clientConfig>"#;
        assert!(parse_ispdb(xml).is_none());
    }

    #[test]
    fn ispdb_parses() {
        let xml = r#"<clientConfig><emailProvider>
            <incomingServer type="pop3"><hostname>pop.x.org</hostname><port>995</port><socketType>SSL</socketType></incomingServer>
            <incomingServer type="imap"><hostname>imap.x.org</hostname><port>993</port><socketType>SSL</socketType></incomingServer>
            <outgoingServer type="smtp"><hostname>smtp.x.org</hostname><port>587</port><socketType>STARTTLS</socketType></outgoingServer>
        </emailProvider></clientConfig>"#;
        let (i, s) = parse_ispdb(xml).unwrap();
        assert_eq!(i.host, "imap.x.org");
        assert_eq!(i.security, Security::Tls);
        assert_eq!(s.port, 587);
        assert_eq!(s.security, Security::StartTls);
    }
}
