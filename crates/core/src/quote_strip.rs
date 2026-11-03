//! Extracts the "new" part of a message (without quoted history or signatures) so long
//! email threads can be shown as chat bubbles.

use regex::Regex;
use std::sync::LazyLock;

static REPLY_HEADER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)^\s*(
            on\s.{0,250}\swrote:\s*$ |
            el\s.{0,250}\sescribió:\s*$ |
            el\s.{0,250}\sescribio:\s*$ |
            em\s.{0,250}\sescreveu:\s*$ |
            le\s.{0,250}\sa\sécrit\s?:\s*$ |
            am\s.{0,250}\sschrieb\s?.{0,80}:\s*$ |
            il\s.{0,250}\sha\sscritto:\s*$ |
            -{2,}\s*(original\smessage|mensaje\soriginal|message\sd'origine|ursprüngliche\snachricht)\s*-{2,}.*$ |
            -{2,}\s*(original|原始邮件|邮件原文)\s*-{2,}\s*$ |
            -{2,}\s*(replied\smessage|回复的原邮件)\s*-{2,}\s*$ |
            -{2,}\s*(forwarded\smessage|mensaje\sreenviado)\s*-{2,}.*$ |
            _{20,}\s*$
        )",
    )
    .unwrap()
});

// Outlook style header block: "From: X" followed within a few lines by "Sent:/Date:".
static OUTLOOK_FROM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^\s*\*?(from|de|von|发件人)\s*[:：]\*?\s*\S").unwrap());
static OUTLOOK_SENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\s*\*?(sent|send\stime|date|enviado|enviado el|fecha|gesendet|envoyé|发送时间|日期)\s*[:：]\*?\s*\S").unwrap()
});

/// Number of leading `lines` that make up the new part of a message: everything before the
/// quoted history, minus a trailing "-- " signature and mobile footers.
fn kept_lines(lines: &[&str]) -> usize {
    let mut cut = lines.len();
    for i in 0..lines.len() {
        let line = lines[i];
        // Reply headers sometimes wrap over two lines: "On Mon, ... Ana <a@b.c>\nwrote:".
        let joined = lines.get(i + 1).map(|next| format!("{line} {next}"));
        if REPLY_HEADER.is_match(line) {
            cut = i;
            break;
        }
        if let Some(j) = &joined {
            if !line.trim().is_empty() && REPLY_HEADER.is_match(j) && lines[i + 1].trim().len() < 40 {
                cut = i;
                break;
            }
        }
        if OUTLOOK_FROM.is_match(line)
            && lines[i + 1..lines.len().min(i + 5)].iter().any(|l| OUTLOOK_SENT.is_match(l))
        {
            cut = i;
            break;
        }
        // A trailing block of "> quoted" lines.
        if line.trim_start().starts_with('>')
            && lines[i..].iter().all(|l| l.trim().is_empty() || l.trim_start().starts_with('>'))
        {
            cut = i;
            break;
        }
    }

    let mut kept = cut;
    // Standard signature delimiter "-- ".
    if let Some(pos) = lines[..kept].iter().rposition(|l| *l == "-- " || *l == "--") {
        kept = pos;
    }
    // Mobile footers.
    while kept > 0 {
        let t = lines[kept - 1].trim().to_lowercase();
        if t.is_empty()
            || t.starts_with("sent from my")
            || t.starts_with("enviado desde mi")
            || t.starts_with("get outlook for")
            || t.starts_with("obtener outlook para")
        {
            kept -= 1;
        } else {
            break;
        }
    }
    kept
}

/// Returns the reply-only part of a plain text message. Falls back to the whole text when
/// stripping would leave nothing (e.g. a bare forward).
pub fn strip_quotes(text: &str) -> String {
    let text = text.replace("\r\n", "\n");
    let lines: Vec<&str> = text.lines().collect();
    let kept = kept_lines(&lines);
    let result = lines[..kept].join("\n").trim().to_string();
    if result.is_empty() {
        text.trim().to_string()
    } else {
        result
    }
}

fn clean_line(raw: &str) -> String {
    static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t\r\n\u{a0}]+").unwrap());
    let s = html_escape::decode_html_entities(raw);
    SPACES.replace_all(&s, " ").trim().to_string()
}

/// Splits HTML into visual text lines (as `html_to_text` would), each with the byte offset
/// in `html` where it starts, so a cut found on the text can be applied to the HTML.
fn html_lines(html: &str) -> Vec<(usize, String)> {
    static TAG: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?s)<!--.*?-->|<(/?)([a-zA-Z][a-zA-Z0-9]*)[^>]*>").unwrap());
    let lower = html.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut start = 0;
    let mut pos = 0;
    while let Some(c) = TAG.captures_at(html, pos) {
        let m = c.get(0).unwrap();
        buf.push_str(&html[pos..m.start()]);
        pos = m.end();
        let Some(name) = c.get(2) else { continue }; // comment
        let name = name.as_str().to_ascii_lowercase();
        let closing = !c[1].is_empty();
        if !closing && matches!(name.as_str(), "head" | "style" | "script" | "title") {
            pos = lower[pos..]
                .find(&format!("</{name}"))
                .and_then(|e| lower[pos + e..].find('>').map(|g| pos + e + g + 1))
                .unwrap_or(html.len());
            continue;
        }
        let block = matches!(
            name.as_str(),
            "p" | "div" | "li" | "tr" | "table" | "ul" | "ol" | "blockquote" | "hr" | "pre"
                | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
        );
        if name == "br" || (block && closing) {
            out.push((start, clean_line(&buf)));
            buf.clear();
            start = m.end();
        } else if block && !clean_line(&buf).is_empty() {
            out.push((start, clean_line(&buf)));
            buf.clear();
            start = m.start();
        }
    }
    buf.push_str(&html[pos..]);
    if !clean_line(&buf).is_empty() {
        out.push((start, clean_line(&buf)));
    }
    out
}

/// The HTML of only the new part of a message: quoted history (known containers or a reply
/// header found in the text), marked signatures and mobile footers are cut off. The result
/// may have unclosed tags; it is meant to go through the sanitizer.
pub fn cut_html_quotes(html: &str) -> String {
    static SIGNATURE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?is)<div[^>]*(class="[^"]*gmail_signature|data-smartmail="gmail_signature"|id="Signature")[^>]*>.*"#)
            .unwrap()
    });
    let mut base = strip_html_quotes(html);
    if let Some(m) = SIGNATURE.find(&base) {
        if !html_to_text(&base[..m.start()]).is_empty() {
            base.truncate(m.start());
        }
    }
    let lines = html_lines(&base);
    let texts: Vec<&str> = lines.iter().map(|(_, t)| t.as_str()).collect();
    let kept = kept_lines(&texts);
    if texts[..kept].iter().all(|t| t.is_empty()) {
        return base; // nothing new (bare forward): keep everything
    }
    match lines.get(kept) {
        Some((offset, _)) => base[..*offset].to_string(),
        None => base,
    }
}

/// Removes quoted history containers that mail clients wrap around replies in HTML.
pub fn strip_html_quotes(html: &str) -> String {
    static CONTAINERS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
        [
            // Gmail
            r#"(?is)<div[^>]*class="[^"]*gmail_quote[^"]*".*"#,
            // Outlook desktop / web
            r#"(?is)<div[^>]*id="(divRplyFwdMsg|appendonsend)".*"#,
            r#"(?is)<hr[^>]*id="stopSpelling".*"#,
            // Apple Mail / Thunderbird
            r#"(?is)<blockquote[^>]*type="cite".*"#,
            r#"(?is)<div[^>]*class="[^"]*(moz-cite-prefix|yahoo_quoted)[^"]*".*"#,
        ]
        .iter()
        .map(|r| Regex::new(r).unwrap())
        .collect()
    });
    let mut out = html.to_string();
    for re in CONTAINERS.iter() {
        if let Some(m) = re.find(&out) {
            if m.start() > 0 {
                out.truncate(m.start());
            }
        }
    }
    out
}

/// Simple HTML → text conversion good enough for previews and chat bubbles.
pub fn html_to_text(html: &str) -> String {
    static DROP: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<(head|style|script|title)[^>]*>.*?</(head|style|script|title)>").unwrap());
    static BREAKS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)<br\s*/?>|</(p|div|tr|h[1-6]|li|blockquote|table)>").unwrap());
    static LI: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)<li[^>]*>").unwrap());
    static TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<[^>]*>").unwrap());
    static BLANKS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n[ \t\u{a0}]*(\n[ \t\u{a0}]*)+").unwrap());
    static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t\u{a0}]+").unwrap());

    let s = DROP.replace_all(html, "");
    let s = BREAKS.replace_all(&s, "\n");
    let s = LI.replace_all(&s, "• ");
    let s = TAGS.replace_all(&s, "");
    let s = html_escape::decode_html_entities(&s).replace('\r', "");
    let s = SPACES.replace_all(&s, " ");
    let s = s.lines().map(str::trim).collect::<Vec<_>>().join("\n");
    BLANKS.replace_all(&s, "\n\n").trim().to_string()
}

/// Single-line preview for message lists.
pub fn preview(text: &str, max_chars: usize) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        collapsed
    } else {
        let mut s: String = collapsed.chars().take(max_chars).collect();
        s.push('…');
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gmail_english() {
        let t = "Sounds good!\n\nOn Mon, Oct 20, 2026 at 10:00 AM Ana <ana@x.com> wrote:\n> Shall we meet?\n> -- \n> Ana";
        assert_eq!(strip_quotes(t), "Sounds good!");
    }

    #[test]
    fn gmail_spanish_wrapped() {
        let t = "Perfecto, nos vemos.\n\nEl lun, 20 oct 2026 a las 10:00, Ana Pérez (<ana@x.com>)\nescribió:\n> ¿Nos juntamos?";
        assert_eq!(strip_quotes(t), "Perfecto, nos vemos.");
    }

    #[test]
    fn outlook_header_block() {
        let t = "Adjunto el informe.\n\nSaludos\nJuan\n\nFrom: Ana <ana@x.com>\nSent: Monday, October 20, 2026 10:00\nTo: Juan\nSubject: Informe\n\nHola Juan...";
        assert_eq!(strip_quotes(t), "Adjunto el informe.\n\nSaludos\nJuan");
    }

    #[test]
    fn outlook_spanish_separator() {
        let t = "Ok\n________________________________\nDe: Ana\nEnviado: lunes\nPara: Juan";
        assert_eq!(strip_quotes(t), "Ok");
    }

    #[test]
    fn signature_and_mobile_footer() {
        let t = "Llego en 5\n\nEnviado desde mi iPhone";
        assert_eq!(strip_quotes(t), "Llego en 5");
        let t = "Hola\n-- \nJuan Pérez\nGerente";
        assert_eq!(strip_quotes(t), "Hola");
    }

    #[test]
    fn inline_quotes_kept_when_interleaved() {
        let t = "> pregunta 1\nrespuesta 1\n> pregunta 2\nrespuesta 2";
        assert_eq!(strip_quotes(t), t);
    }

    #[test]
    fn bare_forward_falls_back() {
        let t = "---------- Forwarded message ---------\nFrom: X\nDate: Y\n\nContenido";
        assert_eq!(strip_quotes(t), t);
    }

    #[test]
    fn html_quote_containers() {
        let h = r#"<div>Gracias!</div><div class="gmail_quote"><div>El lun escribió:</div><blockquote>viejo</blockquote></div>"#;
        assert_eq!(html_to_text(&strip_html_quotes(h)), "Gracias!");
        let o = r#"<p>Listo</p><hr style="x" id="stopSpelling"><div id="divRplyFwdMsg">De: Ana</div>"#;
        assert_eq!(html_to_text(&strip_html_quotes(o)), "Listo");
    }

    #[test]
    fn html_text_conversion() {
        let h = "<html><head><style>p{}</style></head><body><p>Hola&nbsp;<b>mundo</b></p><ul><li>uno</li><li>dos</li></ul>&amp; fin</body></html>";
        assert_eq!(html_to_text(h), "Hola mundo\n• uno\n• dos\n& fin");
    }

    #[test]
    fn foxmail_original_separator() {
        let t = "Hi Juan\n\nWe confirm.\n\n------------------ Original ------------------\nFrom:  Juan <juan@example.com>\nSend time:  Monday, January 5, 2026 9:00 AM\nTo:  Ana\n\nHi Ana,";
        assert_eq!(strip_quotes(t), "Hi Juan\n\nWe confirm.");
        let t = "Ok\n\nFrom: Juan <juan@example.com>\nSend time: Saturday\nSubject: RE: x";
        assert_eq!(strip_quotes(t), "Ok");
        let t = "好的\n\n发件人: Juan\n发送时间: 2026年1月5日\n主题: x";
        assert_eq!(strip_quotes(t), "好的");
    }

    #[test]
    fn netease_replied_message() {
        let t = "Thanks\nBest regards\n\n---- Replied Message ----\nFrom\tJuan <juan@example.com>\nDate\t01/05/2026 09:00\n\nHi Ana,";
        assert_eq!(strip_quotes(t), "Thanks\nBest regards");
        let t = "好的\n\n---- 回复的原邮件 ----\n发件人\tJuan";
        assert_eq!(strip_quotes(t), "好的");
        let h = r#"<div>Thanks</div><div>Best regards</div><table><tr><td>Ana</td></tr></table><div style="font-size:12px">---- Replied Message ----</div><table><tr><td>From</td><td><a href="mailto:juan@example.com">Juan&lt;juan@example.com&gt;</a></td></tr><tr><td>Date</td><td>01/05/2026 09:00</td></tr></table><div>Hi Ana,</div>"#;
        let cut = cut_html_quotes(h);
        assert!(cut.contains("Best regards"));
        assert!(!cut.contains("Replied Message"));
        assert!(!cut.contains("Hi Ana"));
    }

    #[test]
    fn html_cut_on_text_header() {
        let h = r#"<div>Hi Juan</div><ol><li>One</li><li>Two</li></ol><div><br></div><div>------------------ Original ------------------</div><div style="background:#efefef"><b>From:</b>&nbsp;Juan</div><div><b>Send time:</b>&nbsp;Sat</div><div>Hi Ana,</div>"#;
        let cut = cut_html_quotes(h);
        assert!(cut.contains("<li>Two</li>"));
        assert!(!cut.contains("Original"));
        assert!(!cut.contains("Hi Ana"));
        let o = r#"<p class=MsoNormal>Listo, <b>gracias</b></p><p>&nbsp;</p><div style="border-top:solid #E1E1E1 1pt"><p><b>From:</b> Ana &lt;a@x.cl&gt;<br><b>Sent:</b> Monday<br><b>To:</b> Juan</p></div><p>viejo</p>"#;
        let cut = cut_html_quotes(o);
        assert_eq!(html_to_text(&cut), "Listo, gracias");
        assert!(!cut.contains("viejo"));
    }

    #[test]
    fn html_cut_keeps_everything_without_header() {
        let h = "<p>Uno</p><table><tr><td>a</td><td>b</td></tr></table><p>Fin</p>";
        assert_eq!(cut_html_quotes(h), h);
        let fwd = "<div>---------- Forwarded message ---------</div><div>From: X</div><div>Contenido</div>";
        assert_eq!(cut_html_quotes(fwd), fwd);
    }

    #[test]
    fn html_cut_signature_container() {
        let h = r#"<div>Nos vemos</div><div class="gmail_signature">Juan<br>Gerente</div>"#;
        assert_eq!(html_to_text(&cut_html_quotes(h)), "Nos vemos");
    }

    #[test]
    fn preview_truncates() {
        assert_eq!(preview("a  b\n\nc", 10), "a b c");
        assert_eq!(preview("abcdefghijkl", 5), "abcde…");
    }
}
