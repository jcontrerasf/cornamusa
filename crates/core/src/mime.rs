//! Parsing incoming messages (mail-parser) and building outgoing ones (lettre).

use crate::error::{Error, Result};
use crate::model::{Account, AttachmentInfo, EmailAddress, OutgoingMessage};
use crate::quote_strip::{cut_html_quotes, html_to_text, preview, strip_quotes};
use crate::sanitize::{sanitize_chat_html, sanitize_email_html};
use crate::threading::{normalize_id, parse_id_list};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use lettre::message::header::ContentType;
use lettre::message::{Attachment, Mailbox, MultiPart, SinglePart};
use mail_parser::{Address, HeaderValue, Message, MessageParser, MimeHeaders};

#[derive(Debug, Clone, Default)]
pub struct ParsedHeaders {
    pub message_id: Option<String>,
    pub subject: String,
    pub from: Option<EmailAddress>,
    pub to: Vec<EmailAddress>,
    pub cc: Vec<EmailAddress>,
    pub reply_to: Vec<EmailAddress>,
    pub date: Option<i64>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub has_attachments: bool,
}

#[derive(Debug, Clone)]
pub struct ParsedBody {
    pub headers: ParsedHeaders,
    pub html: Option<String>,
    pub text: Option<String>,
    pub remote_blocked: bool,
    pub attachments: Vec<AttachmentInfo>,
    pub chat_text: String,
    /// Sanitized (strict profile) HTML of the new part, when the message has HTML.
    pub chat_html: Option<String>,
    pub preview: String,
}

fn addresses(a: Option<&Address>) -> Vec<EmailAddress> {
    let Some(a) = a else { return vec![] };
    let list: Vec<&mail_parser::Addr> = match a {
        Address::List(l) => l.iter().collect(),
        Address::Group(g) => g.iter().flat_map(|g| g.addresses.iter()).collect(),
    };
    list.into_iter()
        .filter_map(|addr| {
            Some(EmailAddress {
                name: addr.name().map(|n| n.trim().to_string()).filter(|n| !n.is_empty()),
                email: addr.address()?.trim().to_string(),
            })
        })
        .collect()
}

fn id_list(v: &HeaderValue) -> Vec<String> {
    match v {
        HeaderValue::Text(t) => parse_id_list(t),
        HeaderValue::TextList(l) => l.iter().map(|s| normalize_id(s)).filter(|s| !s.is_empty()).collect(),
        _ => vec![],
    }
}

fn headers_of(msg: &Message) -> ParsedHeaders {
    let has_attachments = msg.attachments().any(|p| !is_inline(p))
        || msg
            .content_type()
            .map(|ct| ct.ctype().eq_ignore_ascii_case("multipart") && ct.subtype().is_some_and(|s| s.eq_ignore_ascii_case("mixed")))
            .unwrap_or(false)
            && msg.parts.len() <= 1; // header-only parse: rely on the content type
    ParsedHeaders {
        message_id: msg.message_id().map(normalize_id),
        subject: msg.subject().unwrap_or("").trim().to_string(),
        from: addresses(msg.from()).into_iter().next(),
        to: addresses(msg.to()),
        cc: addresses(msg.cc()),
        reply_to: addresses(msg.reply_to()),
        date: msg.date().map(|d| d.to_timestamp()),
        in_reply_to: id_list(msg.in_reply_to()).into_iter().next(),
        references: id_list(msg.references()),
        has_attachments,
    }
}

fn is_inline(part: &mail_parser::MessagePart) -> bool {
    let disposition_inline = part
        .content_disposition()
        .map(|d| d.ctype().eq_ignore_ascii_case("inline"))
        .unwrap_or(false);
    part.content_id().is_some() && (disposition_inline || part.attachment_name().is_none())
}

/// Parse only the header block (used during sync with `BODY.PEEK[HEADER]`).
pub fn parse_headers(raw: &[u8]) -> ParsedHeaders {
    MessageParser::default().parse_headers(raw).map(|m| headers_of(&m)).unwrap_or_default()
}

fn mime_of(part: &mail_parser::MessagePart) -> String {
    part.content_type()
        .map(|ct| format!("{}/{}", ct.ctype(), ct.subtype().unwrap_or("octet-stream")).to_ascii_lowercase())
        .unwrap_or_else(|| "application/octet-stream".into())
}

/// Replace `cid:` references with inline `data:` URIs for embedded images.
fn inline_cid_images(html: &str, msg: &Message) -> String {
    let mut out = html.to_string();
    for part in msg.attachments() {
        let (Some(cid), mime) = (part.content_id(), mime_of(part)) else { continue };
        if !mime.starts_with("image/") || mime.contains("svg") {
            continue;
        }
        let cid = normalize_id(cid);
        let data = format!("data:{mime};base64,{}", STANDARD.encode(part.contents()));
        out = out.replace(&format!("cid:{cid}"), &data);
    }
    out
}

/// Fully parse a message for display. `allow_remote` controls remote images/CSS.
pub fn parse_full(raw: &[u8], allow_remote: bool) -> Result<ParsedBody> {
    let msg = MessageParser::default()
        .parse(raw)
        .ok_or_else(|| Error::Message("Mensaje MIME ilegible".into()))?;
    let headers = headers_of(&msg);

    let raw_html = msg.body_html(0).map(|h| h.into_owned());
    // mail-parser synthesizes text from HTML (and vice versa) when one is missing; prefer the
    // real text part when it exists.
    let has_text_part = msg.text_part(0).map(|p| !p.is_content_type("text", "html")).unwrap_or(false);
    let text = if has_text_part { msg.body_text(0).map(|t| t.into_owned()) } else { None };
    let has_html_part = msg.html_part(0).map(|p| p.is_content_type("text", "html")).unwrap_or(false);

    let (html, remote_blocked) = match raw_html.as_ref().filter(|_| has_html_part) {
        Some(h) => {
            let s = sanitize_email_html(h, allow_remote);
            (Some(inline_cid_images(&s.html, &msg)), s.remote_blocked)
        }
        None => (None, false),
    };

    let cut_html = raw_html.as_deref().map(cut_html_quotes);
    let chat_text = match (&text, &cut_html) {
        (Some(t), _) if !t.trim().is_empty() => strip_quotes(t),
        (_, Some(h)) => strip_quotes(&html_to_text(h)),
        _ => String::new(),
    };
    // Formatted version of the new part, for chat bubbles. Dropped when it's empty or clearly
    // longer than the text version (the HTML cut missed a quote the text cut found).
    let chat_html = cut_html.filter(|_| has_html_part).and_then(|h| {
        let h = inline_cid_images(&sanitize_chat_html(&h, allow_remote), &msg);
        let len = html_to_text(&h).chars().count();
        let text_len = chat_text.chars().count();
        let empty = len == 0 && !h.contains("<img");
        (!empty && len <= text_len + text_len / 2 + 200).then_some(h)
    });
    let preview_src = if chat_text.is_empty() { text.clone().unwrap_or_default() } else { chat_text.clone() };

    let attachments = msg
        .attachments()
        .enumerate()
        .map(|(index, part)| {
            let filename = part
                .attachment_name()
                .map(sanitize_filename)
                .unwrap_or_else(|| format!("adjunto-{}", index + 1));
            let mime_type = mime_of(part);
            AttachmentInfo {
                index,
                kind: crate::attachments::classify(&filename, &mime_type),
                can_open: crate::attachments::can_open_externally(&filename, &mime_type),
                filename,
                mime_type,
                size: part.contents().len(),
                inline: is_inline(part),
            }
        })
        .collect::<Vec<_>>();

    let mut headers = headers;
    headers.has_attachments = attachments.iter().any(|a| !a.inline);

    Ok(ParsedBody {
        headers,
        html,
        text,
        remote_blocked,
        attachments,
        preview: preview(&preview_src, 180),
        chat_text,
        chat_html,
    })
}

/// Bytes of the attachment at `index` (as listed by `parse_full`).
pub fn attachment(raw: &[u8], index: usize) -> Result<(String, String, Vec<u8>)> {
    let msg = MessageParser::default()
        .parse(raw)
        .ok_or_else(|| Error::Message("Mensaje MIME ilegible".into()))?;
    let part = msg
        .attachments()
        .nth(index)
        .ok_or_else(|| Error::NotFound(format!("adjunto {index}")))?;
    let name = part
        .attachment_name()
        .map(sanitize_filename)
        .unwrap_or_else(|| format!("adjunto-{}", index + 1));
    Ok((name, mime_of(part), part.contents().to_vec()))
}

/// Make an attachment name safe to write to disk: no path separators, no traversal,
/// no control or reserved characters.
pub fn sanitize_filename(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cleaned: String = base
        .chars()
        .map(|c| if c.is_control() || r#"<>:"|?*"#.contains(c) { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.').trim_end_matches(['.', ' ']).to_string();
    let reserved = ["CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "LPT1", "LPT2"];
    let stem = cleaned.split('.').next().unwrap_or("").to_ascii_uppercase();
    if cleaned.is_empty() || reserved.contains(&stem.as_str()) {
        format!("adjunto_{cleaned}")
    } else {
        cleaned.chars().take(200).collect()
    }
}

fn mailbox(a: &EmailAddress) -> Result<Mailbox> {
    let addr = a
        .email
        .trim()
        .parse()
        .map_err(|e| Error::Message(format!("Dirección inválida «{}»: {e}", a.email)))?;
    Ok(Mailbox::new(a.name.clone().filter(|n| !n.is_empty()), addr))
}

pub struct Built {
    pub message: lettre::Message,
    pub message_id: String,
}

/// Build the outgoing MIME message (multipart/alternative text+html, plus attachments).
/// An attachment already in memory (e.g. taken from a forwarded message).
pub struct MemoryAttachment {
    pub filename: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

/// Non-inline attachments of a raw message, for forwarding.
pub fn forwardable_attachments(raw: &[u8]) -> Result<Vec<MemoryAttachment>> {
    let msg = MessageParser::default()
        .parse(raw)
        .ok_or_else(|| Error::Message("Mensaje MIME ilegible".into()))?;
    Ok(msg
        .attachments()
        .enumerate()
        .filter(|(_, p)| !is_inline(p))
        .map(|(i, p)| MemoryAttachment {
            filename: p.attachment_name().map(sanitize_filename).unwrap_or_else(|| format!("adjunto-{}", i + 1)),
            mime_type: mime_of(p),
            bytes: p.contents().to_vec(),
        })
        .collect())
}

pub fn build_outgoing(
    account: &Account,
    out: &OutgoingMessage,
    extra: Vec<MemoryAttachment>,
    require_recipients: bool,
) -> Result<Built> {
    if require_recipients && out.to.is_empty() && out.cc.is_empty() && out.bcc.is_empty() {
        return Err(Error::Message("Agrega al menos un destinatario".into()));
    }
    let domain = account.email.rsplit_once('@').map(|(_, d)| d).unwrap_or("cornamusa.local");
    let message_id = format!("{}@{domain}", uuid::Uuid::new_v4());

    let from = EmailAddress { name: Some(account.display_name.clone()), email: account.email.clone() };
    let mut b = lettre::Message::builder()
        .from(mailbox(&from)?)
        .subject(out.subject.clone())
        .message_id(Some(format!("<{message_id}>")))
        .date_now()
        .user_agent("Cornamusa".into());
    for a in &out.to {
        b = b.to(mailbox(a)?);
    }
    for a in &out.cc {
        b = b.cc(mailbox(a)?);
    }
    for a in &out.bcc {
        b = b.bcc(mailbox(a)?);
    }
    if out.to.is_empty() && out.cc.is_empty() && out.bcc.is_empty() {
        // Drafts may have no recipients yet; lettre still needs an SMTP envelope to build the
        // message. The envelope is not part of the formatted message, it's never sent.
        let me: lettre::Address = account
            .email
            .parse()
            .map_err(|e| Error::Message(format!("Dirección inválida «{}»: {e}", account.email)))?;
        let envelope = lettre::address::Envelope::new(Some(me.clone()), vec![me]).map_err(|e| Error::Message(e.to_string()))?;
        b = b.envelope(envelope);
    }
    if let Some(parent) = &out.in_reply_to {
        b = b.in_reply_to(format!("<{}>", normalize_id(parent)));
    }
    if !out.references.is_empty() {
        let refs = out.references.iter().map(|r| format!("<{}>", normalize_id(r))).collect::<Vec<_>>().join(" ");
        b = b.references(refs);
    }

    let html_doc = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"></head><body>{}</body></html>",
        out.html
    );
    let alternative = MultiPart::alternative()
        .singlepart(SinglePart::plain(out.text.clone()))
        .singlepart(SinglePart::html(html_doc));

    let octet = || ContentType::parse("application/octet-stream").unwrap();
    let message = if out.attachments.is_empty() && extra.is_empty() {
        b.multipart(alternative)
    } else {
        let mut mixed = MultiPart::mixed().multipart(alternative);
        for att in &out.attachments {
            let path = std::path::Path::new(&att.path);
            let bytes = std::fs::read(path)?;
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "adjunto".into());
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            let ct = ContentType::parse(mime.essence_str()).unwrap_or_else(|_| octet());
            mixed = mixed.singlepart(Attachment::new(name).body(bytes, ct));
        }
        for att in extra {
            let ct = ContentType::parse(&att.mime_type).unwrap_or_else(|_| octet());
            mixed = mixed.singlepart(Attachment::new(att.filename).body(att.bytes, ct));
        }
        b.multipart(mixed)
    }
    .map_err(|e| Error::Message(e.to_string()))?;

    Ok(Built { message, message_id })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Provider, Security, ServerConfig};

    const SAMPLE: &[u8] = b"From: =?utf-8?q?Ana_P=C3=A9rez?= <ana@example.com>\r\n\
To: juan@example.com, \"Luis\" <luis@example.com>\r\n\
Subject: =?utf-8?b?UmU6IFJldW5pw7Nu?=\r\n\
Date: Mon, 20 Oct 2026 10:00:00 +0000\r\n\
Message-ID: <m2@example.com>\r\n\
In-Reply-To: <m1@example.com>\r\n\
References: <m0@example.com> <m1@example.com>\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"XX\"\r\n\
\r\n\
--XX\r\n\
Content-Type: multipart/alternative; boundary=\"YY\"\r\n\
\r\n\
--YY\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Nos vemos a las 5.\r\n\
\r\n\
On Sun, Oct 19, 2026 Juan <juan@example.com> wrote:\r\n\
> =C2=BFA qu=C3=A9 hora?\r\n\
--YY\r\n\
Content-Type: text/html; charset=utf-8\r\n\
\r\n\
<p>Nos vemos a las 5.</p><img src=\"https://tracker.example/p.gif\"><img src=\"cid:logo@x\"><script>alert(1)</script>\r\n\
--YY--\r\n\
--XX\r\n\
Content-Type: image/png\r\n\
Content-ID: <logo@x>\r\n\
Content-Disposition: inline\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
iVBORw0KGgo=\r\n\
--XX\r\n\
Content-Type: application/pdf; name=\"../../etc/informe.pdf\"\r\n\
Content-Disposition: attachment; filename=\"../../etc/informe.pdf\"\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
JVBERi0xLjQK\r\n\
--XX--\r\n";

    #[test]
    fn parses_headers_and_threading_fields() {
        let h = parse_headers(SAMPLE);
        assert_eq!(h.subject, "Re: Reunión");
        assert_eq!(h.from.as_ref().unwrap().name.as_deref(), Some("Ana Pérez"));
        assert_eq!(h.to.len(), 2);
        assert_eq!(h.message_id.as_deref(), Some("m2@example.com"));
        assert_eq!(h.in_reply_to.as_deref(), Some("m1@example.com"));
        assert_eq!(h.references, vec!["m0@example.com", "m1@example.com"]);
        assert!(h.has_attachments);
    }

    #[test]
    fn parses_full_body() {
        let b = parse_full(SAMPLE, false).unwrap();
        let html = b.html.unwrap();
        assert!(!html.contains("script"));
        assert!(!html.contains("tracker.example"));
        assert!(html.contains("data:image/png;base64,"));
        assert!(b.remote_blocked);
        assert_eq!(b.chat_text, "Nos vemos a las 5.");
        assert_eq!(b.attachments.len(), 2);
        let pdf = b.attachments.iter().find(|a| !a.inline).unwrap();
        assert_eq!(pdf.filename, "informe.pdf");
        assert_eq!(pdf.mime_type, "application/pdf");
        let (name, _, bytes) = attachment(SAMPLE, pdf.index).unwrap();
        assert_eq!(name, "informe.pdf");
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn filenames() {
        assert_eq!(sanitize_filename("..\\..\\win.ini"), "win.ini");
        assert_eq!(sanitize_filename("a:b?.txt"), "a_b_.txt");
        assert_eq!(sanitize_filename("..."), "adjunto_");
        assert_eq!(sanitize_filename("CON.txt"), "adjunto_CON.txt");
    }

    #[test]
    fn builds_reply_with_attachment() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notas.txt");
        std::fs::write(&path, "hola").unwrap();
        let server = ServerConfig { host: "x".into(), port: 993, security: Security::Tls };
        let account = Account {
            id: "a".into(),
            provider: Provider::Imap,
            email: "yo@example.com".into(),
            display_name: "Yo".into(),
            username: "yo@example.com".into(),
            imap: server.clone(),
            smtp: server,
            color: "#000".into(),
            auth_user: None,
        };
        let out = OutgoingMessage {
            account_id: "a".into(),
            to: vec![EmailAddress { name: Some("Ana".into()), email: "ana@example.com".into() }],
            cc: vec![],
            bcc: vec![],
            subject: "Re: Reunión".into(),
            html: "<p>Listo</p>".into(),
            text: "Listo".into(),
            attachments: vec![crate::model::OutgoingAttachment { path: path.to_string_lossy().into() }],
            in_reply_to: Some("m2@example.com".into()),
            references: vec!["m0@example.com".into(), "m2@example.com".into()],
            forward_attachments_of: None,
        };
        let built = build_outgoing(&account, &out, vec![], true).unwrap();
        let raw = String::from_utf8(built.message.formatted()).unwrap();
        assert!(raw.contains("In-Reply-To: <m2@example.com>"));
        assert!(raw.contains("References: <m0@example.com> <m2@example.com>"));
        assert!(raw.contains("notas.txt"));
        assert!(raw.contains(&built.message_id));
        // Round trip through our own parser.
        let parsed = parse_full(raw.as_bytes(), false).unwrap();
        assert_eq!(parsed.headers.subject, "Re: Reunión");
        assert_eq!(parsed.attachments.len(), 1);
        assert_eq!(parsed.chat_text, "Listo");
    }

    #[test]
    fn foxmail_style_reply_chat_html() {
        let raw = "From: Ana <ana@example.com>\r\nTo: juan@example.com\r\nSubject: RE: pedido\r\nMIME-Version: 1.0\r\n\
Content-Type: multipart/alternative; boundary=\"b\"\r\n\r\n--b\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n\
Hi Juan\r\n\r\nWe confirm\r\n1. Yes\r\n2. China\r\n\r\n------------------ Original ------------------\r\n\
From:  Juan <juan@example.com>\r\nSend time:  Saturday\r\n\r\nHi Ana,\r\n--b\r\nContent-Type: text/html; charset=utf-8\r\n\r\n\
<div>Hi Juan</div><div>We confirm</div><ol><li>Yes</li><li>China</li></ol><div><br></div>\
<div style=\"font-size:12px\">------------------&nbsp;Original&nbsp;------------------</div>\
<div style=\"background:#efefef\"><div><b>From:</b>&nbsp;Juan</div><div><b>Send time:</b>&nbsp;Saturday</div></div>\
<div>Hi Ana,</div>\r\n--b--\r\n";
        let p = parse_full(raw.as_bytes(), false).unwrap();
        assert_eq!(p.chat_text, "Hi Juan\n\nWe confirm\n1. Yes\n2. China");
        let h = p.chat_html.unwrap();
        assert!(h.contains("<li>China</li>"), "{h}");
        assert!(!h.contains("Original") && !h.contains("Ana") && !h.contains("style"), "{h}");
        assert!(h.ends_with("</ol>"), "{h}");
    }

    #[test]
    fn drafts_without_recipients() {
        let server = ServerConfig { host: "x".into(), port: 993, security: Security::Tls };
        let account = Account {
            id: "a".into(), provider: Provider::Imap, email: "yo@example.com".into(), display_name: "Yo".into(),
            username: "yo".into(), imap: server.clone(), smtp: server, color: "#000".into(),
            auth_user: None,
        };
        let out = OutgoingMessage {
            account_id: "a".into(), to: vec![], cc: vec![], bcc: vec![], subject: "Idea".into(),
            html: "<p>x</p>".into(), text: "x".into(), attachments: vec![], in_reply_to: None,
            references: vec![], forward_attachments_of: None,
        };
        assert!(build_outgoing(&account, &out, vec![], true).is_err());
        let raw = String::from_utf8(build_outgoing(&account, &out, vec![], false).unwrap().message.formatted()).unwrap();
        assert!(!raw.contains("\nTo:"));
        assert!(raw.contains("Subject: Idea"));
    }

    #[test]
    fn forwards_original_attachments() {
        let extra = forwardable_attachments(SAMPLE).unwrap();
        assert_eq!(extra.len(), 1); // the inline logo is not forwarded
        assert_eq!(extra[0].filename, "informe.pdf");
    }
}
