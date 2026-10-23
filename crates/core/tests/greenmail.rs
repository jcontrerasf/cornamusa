//! End-to-end test against a real IMAP/SMTP server over TLS (GreenMail).
//!
//! Skipped unless `CORNAMUSA_IT=1`. See README → "Pruebas de integración".
//! Expects users yo@localhost / ana@localhost (password "secreto") on
//! localhost:3993 (IMAPS) and localhost:3465 (SMTPS), and CORNAMUSA_EXTRA_CA pointing to the
//! CA that signed the server certificate.

use cornamusa_core::imap::{self, Credential};
use cornamusa_core::model::*;
use cornamusa_core::secrets::Secrets;
use cornamusa_core::service::{Event, EventSink, MailService, MoveTarget, PasswordAccountRequest};
use cornamusa_core::store::Store;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Default)]
struct Collect(Mutex<Vec<Event>>);

impl EventSink for Collect {
    fn emit(&self, e: Event) {
        self.0.lock().unwrap().push(e);
    }
}

fn server(port: u16) -> ServerConfig {
    ServerConfig { host: "localhost".into(), port, security: Security::Tls }
}

fn addr(email: &str) -> EmailAddress {
    EmailAddress { name: None, email: email.into() }
}

fn out(account_id: &str, to: &str, subject: &str, text: &str) -> OutgoingMessage {
    OutgoingMessage {
        account_id: account_id.into(),
        to: vec![addr(to)],
        cc: vec![],
        bcc: vec![],
        subject: subject.into(),
        html: format!("<p>{text}</p>"),
        text: text.into(),
        attachments: vec![],
        in_reply_to: None,
        references: vec![],
        forward_attachments_of: None,
    }
}

async fn wait_for(mut f: impl AsyncFnMut() -> bool) -> bool {
    for _ in 0..60 {
        if f().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    false
}

#[tokio::test(flavor = "multi_thread")]
async fn full_flow() {
    if std::env::var("CORNAMUSA_IT").as_deref() != Ok("1") {
        eprintln!("skipped: set CORNAMUSA_IT=1 to run against GreenMail");
        return;
    }

    let _ = tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).with_test_writer().try_init();

    // Start from an empty server (GreenMail's admin API), so reruns are deterministic.
    let api = std::env::var("CORNAMUSA_IT_GREENMAIL_API").unwrap_or_else(|_| "http://127.0.0.1:8080".into());
    reqwest::Client::new().post(format!("{api}/api/service/reset")).send().await.expect("GreenMail reset");
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Create the special folders (GreenMail starts with INBOX only).
    let cred = Credential::Password { user: "yo@localhost".into(), password: "secreto".into() };
    let mut s = imap::connect(&server(3993), &cred).await.expect("IMAPS connect");
    for f in ["Sent", "Drafts", "Trash", "Archive"] {
        let _ = s.create(f).await;
    }
    s.logout().await.ok();

    // Plaintext is refused: STARTTLS on the plain IMAP port must fail if not offered.
    let plain = ServerConfig { host: "localhost".into(), port: 3143, security: Security::StartTls };
    assert!(imap::connect(&plain, &cred).await.is_err(), "must never fall back to plaintext");

    // XOAUTH2 (the Gmail/Outlook path) over IMAP and SMTP. GreenMail accepts the user's
    // password as the bearer token.
    let oauth = Credential::OAuth { user: "yo@localhost".into(), access_token: "secreto".into() };
    imap::connect(&server(3993), &oauth).await.expect("IMAP XOAUTH2").logout().await.ok();
    cornamusa_core::smtp::verify(&server(3465), &oauth).await.expect("SMTP XOAUTH2");
    let wrong = Credential::OAuth { user: "yo@localhost".into(), access_token: "expirado".into() };
    assert!(imap::connect(&server(3993), &wrong).await.is_err(), "bad token rejected");

    let sink = Arc::new(Collect::default());
    let svc = MailService::with_secrets(Store::open_memory().await.unwrap(), sink.clone(), Secrets::memory());
    let yo = svc
        .add_password_account(PasswordAccountRequest {
            email: "yo@localhost".into(),
            display_name: "Yo".into(),
            username: None,
            password: "secreto".into(),
            imap: server(3993),
            smtp: server(3465),
        })
        .await
        .expect("add yo");
    let ana = svc
        .add_password_account(PasswordAccountRequest {
            email: "ana@localhost".into(),
            display_name: "Ana".into(),
            username: None,
            password: "secreto".into(),
            imap: server(3993),
            smtp: server(3465),
        })
        .await
        .expect("add ana");

    // Wrong password is rejected and nothing is stored.
    let bad = svc
        .add_password_account(PasswordAccountRequest {
            email: "ana@localhost".into(),
            display_name: "x".into(),
            username: None,
            password: "malo".into(),
            imap: server(3993),
            smtp: server(3465),
        })
        .await;
    assert!(bad.is_err());

    svc.sync_account(&yo.id).await.expect("initial sync");
    let folders = svc.store().folders(&yo.id).await.unwrap();
    let role = |r: FolderRole| folders.iter().find(|f| f.role == r).unwrap_or_else(|| panic!("{r:?} missing")).clone();
    let (inbox, sent, drafts, archive) = (role(FolderRole::Inbox), role(FolderRole::Sent), role(FolderRole::Drafts), role(FolderRole::Archive));
    assert_eq!(inbox.name, "Recibidos");

    // 1. Ana writes to Yo, with an attachment.
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("menu.txt");
    std::fs::write(&file, "choripán").unwrap();
    let mut first = out(&ana.id, "yo@localhost", "Asado el sábado", "¿Vienes al asado?");
    first.attachments.push(OutgoingAttachment { path: file.to_string_lossy().into() });
    let first_id = svc.send(&first).await.expect("ana sends");

    // The worker's IDLE (or our sync) picks it up.
    assert!(
        wait_for(async || {
            svc.sync_account(&yo.id).await.unwrap();
            !svc.store().threads(&[inbox.id], 10, 0).await.unwrap().is_empty()
        })
        .await
    );
    let threads = svc.store().threads(&[inbox.id], 10, 0).await.unwrap();
    assert_eq!(threads.len(), 1);
    assert!(threads[0].has_attachments);
    let t = threads[0].clone();
    let body = svc.message(t.latest_id, false).await.unwrap();
    assert_eq!(body.chat_text, "¿Vienes al asado?");
    assert_eq!(body.attachments.iter().filter(|a| !a.inline).count(), 1);
    let saved = dir.path().join("saved.txt");
    svc.save_attachment(t.latest_id, body.attachments[0].index, &saved).await.unwrap();
    assert_eq!(std::fs::read_to_string(&saved).unwrap(), "choripán");

    // 2. Yo replies: copy appended to Sent (generic IMAP) and original flagged \Answered.
    let mut reply = out(&yo.id, "ana@localhost", "Re: Asado el sábado", "¡Claro! Llevo el pebre.");
    reply.in_reply_to = Some(first_id.clone());
    reply.references = vec![first_id.clone()];
    let reply_id = svc.send(&reply).await.expect("yo replies");
    assert!(!svc.store().messages(sent.id, 10, 0).await.unwrap().is_empty(), "copy in Sent");

    // 3. Ana answers the reply, Outlook-style (In-Reply-To only, no References).
    svc.sync_account(&ana.id).await.unwrap();
    let mut third = out(&ana.id, "yo@localhost", "RE: Asado el sábado", "Perfecto 🙌");
    third.in_reply_to = Some(reply_id.clone());
    svc.send(&third).await.unwrap();

    assert!(
        wait_for(async || {
            svc.sync_account(&yo.id).await.unwrap();
            svc.store().thread_summaries(&yo.id, &t.thread_id).await.unwrap().len() == 3
        })
        .await,
        "the three messages form one conversation across Inbox and Sent"
    );
    let conv = svc.thread(&yo.id, &t.thread_id, false).await.unwrap();
    let texts: Vec<&str> = conv.iter().map(|m| m.chat_text.as_str()).collect();
    assert_eq!(texts, vec!["¿Vienes al asado?", "¡Claro! Llevo el pebre.", "Perfecto 🙌"]);
    let inbox_threads = svc.store().threads(&[inbox.id], 10, 0).await.unwrap();
    assert_eq!(inbox_threads.len(), 1);
    assert_eq!(inbox_threads[0].message_count, 3);

    // 4. Flags round-trip through the server.
    let rows = svc.thread_rows(&yo.id, &t.thread_id, &[inbox.id]).await.unwrap();
    assert_eq!(rows.len(), 2);
    svc.set_seen(&rows, true).await.unwrap();
    svc.set_flagged(&rows[..1], true).await.unwrap();
    svc.sync_account(&yo.id).await.unwrap();
    let after = svc.store().messages(inbox.id, 10, 0).await.unwrap();
    assert!(after.iter().all(|m| m.seen));
    assert!(after.iter().any(|m| m.flagged));
    let original = after.iter().find(|m| m.message_id.as_deref() == Some(first_id.as_str())).unwrap();
    assert!(original.answered, "original marked as answered after replying");

    // 5. Archive the conversation from the inbox.
    svc.move_messages(&rows, MoveTarget::Archive).await.unwrap();
    assert!(svc.store().messages(inbox.id, 10, 0).await.unwrap().is_empty());
    assert_eq!(svc.store().messages(archive.id, 10, 0).await.unwrap().len(), 2);
    // Still one conversation, now living in Archive + Sent.
    assert_eq!(svc.store().thread_summaries(&yo.id, &t.thread_id).await.unwrap().len(), 3);

    // 6. Drafts: save, replace, delete.
    let mut d = out(&yo.id, "ana@localhost", "Borrador", "versión 1");
    d.to.clear();
    let d1 = svc.save_draft(&d, None).await.unwrap();
    d.text = "versión 2".into();
    let d2 = svc.save_draft(&d, Some(d1)).await.unwrap();
    let ds = svc.store().messages(drafts.id, 10, 0).await.unwrap();
    assert_eq!(ds.len(), 1, "previous draft replaced");
    assert_eq!(ds[0].message_id.as_deref(), Some(d2.as_str()));
    svc.delete_draft(&yo.id, &d2).await.unwrap();
    assert!(svc.store().messages(drafts.id, 10, 0).await.unwrap().is_empty());

    // 7. Search (local FTS, accent-insensitive).
    assert!(!svc.store().search("sabado", Some(&yo.id), 10).await.unwrap().is_empty());

    // 8. Real-time: with the background worker in IDLE, a new message produces a NewMail event.
    sink.0.lock().unwrap().clear();
    tokio::time::sleep(Duration::from_secs(2)).await; // let the worker enter IDLE
    svc.send(&out(&ana.id, "yo@localhost", "¿Y el carbón?", "¿Quién compra el carbón?")).await.unwrap();
    let got = wait_for(async || {
        sink.0.lock().unwrap().iter().any(|e| matches!(e, Event::NewMail(ev) if ev.account_id == yo.id
            && ev.messages.iter().any(|m| m.subject == "¿Y el carbón?")))
    })
    .await;
    assert!(got, "IDLE push produced a new-mail notification");

    // 9. Removing an account drops its data and credentials.
    svc.remove_account(&ana.id).await.unwrap();
    assert_eq!(svc.store().accounts().await.unwrap().len(), 1);
    svc.stop_all();
}
