//! Incoming HTML is hostile. It is cleaned here with ammonia, and the frontend additionally
//! renders it inside a script-less sandboxed iframe with a restrictive CSP.

use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

static REMOTE_SRC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)(src|background|srcset)\s*=\s*["']?\s*(https?:)?//"#).unwrap());
static CSS_REMOTE_URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)url\(\s*["']?\s*(https?:)?//[^)]*\)"#).unwrap());
static CSS_IMPORT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?i)@import[^;]*;?"#).unwrap());
static CSS_DANGEROUS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)(expression\s*\(|behavior\s*:|-moz-binding)"#).unwrap());

pub struct Sanitized {
    pub html: String,
    pub remote_blocked: bool,
}

pub fn has_remote_content(html: &str) -> bool {
    REMOTE_SRC.is_match(html) || CSS_REMOTE_URL.is_match(html) || CSS_IMPORT.is_match(html)
}

fn clean_css(css: &str, allow_remote: bool) -> String {
    let css = CSS_DANGEROUS.replace_all(css, "");
    let css = CSS_IMPORT.replace_all(&css, "");
    if allow_remote {
        css.into_owned()
    } else {
        CSS_REMOTE_URL.replace_all(&css, "none").into_owned()
    }
}

fn is_remote(url: &str) -> bool {
    let u = url.trim_start().to_ascii_lowercase();
    u.starts_with("http:") || u.starts_with("https:") || u.starts_with("//")
}

/// Clean an email's HTML. Remote images and CSS are removed unless `allow_remote` is set,
/// which defeats tracking pixels.
pub fn sanitize_email_html(html: &str, allow_remote: bool) -> Sanitized {
    let remote_present = has_remote_content(html);

    // `<style>` blocks are kept (email layouts depend on them) but their CSS is filtered first.
    static STYLE_BLOCK: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)(<style[^>]*>)(.*?)(</style>)").unwrap());
    let pre = STYLE_BLOCK.replace_all(html, |c: &regex::Captures| {
        format!("{}{}{}", &c[1], clean_css(&c[2], allow_remote), &c[3])
    });

    let tags: HashSet<&str> = [
        "style", "center", "font", "table", "thead", "tbody", "tfoot", "tr", "td", "th", "caption",
        "col", "colgroup", "span", "div", "section", "article", "header", "footer", "main", "nav",
        "img", "picture", "figure", "figcaption", "hr", "br", "u", "s", "strike", "sup", "sub",
        "small", "big", "mark", "abbr", "blockquote", "pre", "code", "kbd", "samp", "q", "cite",
        "dl", "dt", "dd", "ul", "ol", "li", "p", "a", "b", "i", "em", "strong", "h1", "h2", "h3",
        "h4", "h5", "h6", "address", "details", "summary", "wbr",
    ]
    .into_iter()
    .collect();
    let generic: HashSet<&str> = [
        "style", "class", "id", "dir", "lang", "title", "align", "valign", "bgcolor", "width",
        "height", "color", "border", "cellpadding", "cellspacing", "colspan", "rowspan", "face",
        "size", "nowrap",
    ]
    .into_iter()
    .collect();

    let mut builder = ammonia::Builder::default();
    builder
        .tags(tags)
        .rm_clean_content_tags(&["style"])
        .add_clean_content_tags(&["title"])
        .generic_attributes(generic)
        .add_tag_attributes("a", &["href", "name", "target"])
        .add_tag_attributes("img", &["src", "alt", "width", "height"])
        .add_tag_attributes("table", &["background"])
        .add_tag_attributes("td", &["background"])
        .add_url_schemes(&["cid"])
        .link_rel(Some("noopener noreferrer"))
        .strip_comments(true)
        .attribute_filter(move |element, attribute, value| match (element, attribute) {
            (_, "style") => Some(clean_css(value, allow_remote).into()),
            ("img", "src") | (_, "background") if !allow_remote && is_remote(value) => None,
            _ => Some(value.into()),
        });

    Sanitized { html: builder.clean(&pre).to_string(), remote_blocked: remote_present && !allow_remote }
}

/// Strict profile for chat bubbles, whose HTML is inserted directly into the app page (no
/// iframe): only structural/inline formatting survives. No `style`, `class`, `id` or `name`,
/// so a message can't inject CSS or clobber the app's DOM. Trailing empty blocks are dropped.
pub fn sanitize_chat_html(html: &str, allow_remote: bool) -> String {
    let tags: HashSet<&str> = [
        "p", "div", "span", "br", "b", "strong", "i", "em", "u", "s", "strike", "sup", "sub",
        "ul", "ol", "li", "table", "thead", "tbody", "tfoot", "tr", "td", "th", "a", "img",
        "blockquote", "pre", "code", "h1", "h2", "h3", "h4", "h5", "h6", "hr",
    ]
    .into_iter()
    .collect();
    let mut attrs: HashMap<&str, HashSet<&str>> = HashMap::new();
    attrs.insert("a", ["href"].into_iter().collect());
    attrs.insert("img", ["src", "alt", "width", "height"].into_iter().collect());
    attrs.insert("td", ["colspan", "rowspan"].into_iter().collect());
    attrs.insert("th", ["colspan", "rowspan"].into_iter().collect());

    let mut builder = ammonia::Builder::default();
    builder
        .tags(tags)
        .generic_attributes(HashSet::new())
        .tag_attributes(attrs)
        .add_clean_content_tags(&["title"])
        .add_url_schemes(&["cid"])
        .link_rel(Some("noopener noreferrer"))
        .strip_comments(true)
        .attribute_filter(move |element, attribute, value| match (element, attribute) {
            ("img", "src") if !allow_remote && is_remote(value) => None,
            _ => Some(value.into()),
        });
    // Blank paragraphs (Outlook's `<p>&nbsp;</p>` spacers) would double the bubble's margins.
    static EMPTY_P: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)<p>(\s|&nbsp;|\u{a0})*</p>").unwrap());
    let cleaned = builder.clean(html).to_string();
    let mut out = EMPTY_P.replace_all(&cleaned, "").into_owned();

    static TRAILING_EMPTY: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(\s|&nbsp;|\u{a0}|<br\s*/?>|<(p|div|span)>(\s|&nbsp;|\u{a0}|<br\s*/?>)*</(p|div|span)>)+$").unwrap()
    });
    loop {
        let trimmed = TRAILING_EMPTY.replace(&out, "").into_owned();
        if trimmed == out {
            break;
        }
        out = trimmed;
    }
    out
}

/// Sanitize user-authored HTML (signatures). Remote images are allowed since the user wrote it.
pub fn sanitize_signature_html(html: &str) -> String {
    sanitize_email_html(html, true).html
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_profile_is_strict() {
        let h = r#"<style>body{display:none}</style><div id="app" class="x" style="color:red"><b>Hola</b> <a href="https://x.cl" name="y" onclick="z()">link</a><img src="https://t/p.gif"><img src="cid:logo"><form><input></form><script>x()</script></div><ul><li>a</li></ul><table><tr><td colspan="2">c</td></tr></table><p>&nbsp;</p><div><br></div>"#;
        let s = sanitize_chat_html(h, false);
        for bad in ["style", "class", "id=", "name=", "onclick", "script", "form", "input", "https://t/", "display"] {
            assert!(!s.contains(bad), "{bad} in {s}");
        }
        assert!(s.contains("<b>Hola</b>") && s.contains("<li>a</li>") && s.contains(r#"colspan="2""#));
        assert!(s.contains("cid:logo") && s.contains(r#"href="https://x.cl""#));
        assert!(s.ends_with("</table>"), "{s}");
        assert_eq!(sanitize_chat_html("<p>a</p><p>&nbsp;</p><p>b</p>", false), "<p>a</p><p>b</p>");
        assert!(sanitize_chat_html(r#"<img src="https://t/p.gif">"#, true).contains("https://t/p.gif"));
    }

    #[test]
    fn strips_scripts_and_handlers() {
        let s = sanitize_email_html(
            r#"<p onclick="evil()">hola</p><script>alert(1)</script><img src="x" onerror="alert(1)"><a href="javascript:alert(1)">x</a>"#,
            false,
        );
        assert!(!s.html.contains("script"));
        assert!(!s.html.contains("onclick"));
        assert!(!s.html.contains("onerror"));
        assert!(!s.html.contains("javascript"));
        assert!(s.html.contains("hola"));
    }

    #[test]
    fn removes_forms_iframes() {
        let s = sanitize_email_html(
            r#"<form action="https://phish"><input name=p></form><iframe src="https://x"></iframe><object data=x></object>"#,
            false,
        );
        assert!(!s.html.contains("<form"));
        assert!(!s.html.contains("<input"));
        assert!(!s.html.contains("iframe"));
        assert!(!s.html.contains("object"));
    }

    #[test]
    fn blocks_tracking_pixels() {
        let html = r#"<img src="https://track.example/p.gif"><img src="cid:logo"><div style="background:url(https://t.example/a.png)">x</div>"#;
        let s = sanitize_email_html(html, false);
        assert!(s.remote_blocked);
        assert!(!s.html.contains("track.example"));
        assert!(!s.html.contains("t.example"));
        assert!(s.html.contains("cid:logo"));

        let allowed = sanitize_email_html(html, true);
        assert!(!allowed.remote_blocked);
        assert!(allowed.html.contains("track.example"));
    }

    #[test]
    fn filters_style_blocks() {
        let s = sanitize_email_html(
            "<style>@import url(https://x/y.css); .a{background:url('https://t/x.png');width:expression(alert(1))}</style><p class=a>hi</p>",
            false,
        );
        assert!(s.html.contains("<style>"));
        assert!(!s.html.contains("@import"));
        assert!(!s.html.contains("https://t"));
        assert!(!s.html.contains("expression"));
    }

    #[test]
    fn style_cannot_break_out() {
        let s = sanitize_email_html("<style>p{}</style><script>alert(1)</script></style>", false);
        assert!(!s.html.contains("<script"));
    }

    #[test]
    fn drops_document_wrappers() {
        let s = sanitize_email_html("<html><head><title>Newsletter</title><meta charset=utf-8></head><body><p>Hola</p></body></html>", false);
        assert!(!s.html.contains("Newsletter"));
        assert!(s.html.contains("<p>Hola</p>"));
    }

    #[test]
    fn links_get_noopener() {
        let s = sanitize_email_html(r#"<a href="https://example.com">e</a>"#, false);
        assert!(s.html.contains("noopener"));
        assert!(!s.remote_blocked);
    }
}
