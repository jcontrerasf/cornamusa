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
            -{2,}\s*(forwarded\smessage|mensaje\sreenviado)\s*-{2,}.*$ |
            _{20,}\s*$
        )",
    )
    .unwrap()
});

// Outlook style header block: "From: X" followed within a few lines by "Sent:/Date:".
static OUTLOOK_FROM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^\s*\*?(from|de|von)\s*:\*?\s+\S").unwrap());
static OUTLOOK_SENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\s*\*?(sent|date|enviado|enviado el|fecha|gesendet|envoyé)\s*:\*?\s+\S").unwrap()
});

/// Returns the reply-only part of a plain text message. Falls back to the whole text when
/// stripping would leave nothing (e.g. a bare forward).
pub fn strip_quotes(text: &str) -> String {
    let text = text.replace("\r\n", "\n");
    let lines: Vec<&str> = text.lines().collect();
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

    let mut kept: Vec<&str> = lines[..cut].to_vec();
    // Standard signature delimiter "-- ".
    if let Some(pos) = kept.iter().rposition(|l| *l == "-- " || *l == "--") {
        kept.truncate(pos);
    }
    // Mobile footers.
    while let Some(last) = kept.last() {
        let t = last.trim().to_lowercase();
        if t.is_empty()
            || t.starts_with("sent from my")
            || t.starts_with("enviado desde mi")
            || t.starts_with("get outlook for")
            || t.starts_with("obtener outlook para")
        {
            kept.pop();
        } else {
            break;
        }
    }

    let result = kept.join("\n").trim().to_string();
    if result.is_empty() {
        text.trim().to_string()
    } else {
        result
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
    fn preview_truncates() {
        assert_eq!(preview("a  b\n\nc", 10), "a b c");
        assert_eq!(preview("abcdefghijkl", 5), "abcde…");
    }
}
