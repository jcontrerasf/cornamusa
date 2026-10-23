//! Conversation threading based on Message-ID / In-Reply-To / References (RFC 5322).
//!
//! A thread is identified by its root Message-ID. The database layer additionally inherits
//! the thread of any already-known referenced message and merges threads when a parent
//! arrives after its replies (see `Store::assign_thread`).

/// Normalizes a Message-ID: strips angle brackets and surrounding whitespace.
pub fn normalize_id(id: &str) -> String {
    id.trim().trim_start_matches('<').trim_end_matches('>').trim().to_string()
}

/// Parses a References / In-Reply-To header value into individual ids.
pub fn parse_id_list(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = value;
    while let Some(start) = rest.find('<') {
        let Some(end) = rest[start..].find('>') else { break };
        let id = normalize_id(&rest[start..start + end + 1]);
        if !id.is_empty() && !out.contains(&id) {
            out.push(id);
        }
        rest = &rest[start + end + 1..];
    }
    if out.is_empty() {
        out.extend(value.split_whitespace().map(normalize_id).filter(|s| !s.is_empty()));
    }
    out
}

/// Candidate ancestor ids, closest relatives last (References order) with In-Reply-To appended.
pub fn ancestors(references: &[String], in_reply_to: Option<&str>) -> Vec<String> {
    let mut v: Vec<String> = references.to_vec();
    if let Some(p) = in_reply_to.map(normalize_id).filter(|p| !p.is_empty()) {
        if !v.contains(&p) {
            v.push(p);
        }
    }
    v
}

/// The thread id of a message when none of its ancestors are known locally: the oldest
/// ancestor, or the message's own id, or a synthetic id for messages without Message-ID.
pub fn thread_root(references: &[String], in_reply_to: Option<&str>, own_id: Option<&str>, fallback: &str) -> String {
    ancestors(references, in_reply_to)
        .into_iter()
        .next()
        .or_else(|| own_id.map(normalize_id).filter(|s| !s.is_empty()))
        .unwrap_or_else(|| format!("local:{fallback}"))
}

/// Strips reply/forward prefixes in several languages: "Re: RE: Fwd: RV: AW:" ...
pub fn normalize_subject(subject: &str) -> String {
    let mut s = subject.trim();
    loop {
        let lower = s.to_lowercase();
        let mut stripped = false;
        for prefix in ["re", "fw", "fwd", "rv", "res", "aw", "wg", "sv", "tr", "enc", "r"] {
            if let Some(rest) = lower.strip_prefix(prefix) {
                let rest_trim = rest.trim_start();
                // Optional counter like "Re[2]:".
                let rest_trim = if rest_trim.starts_with('[') {
                    rest_trim.find(']').map(|i| &rest_trim[i + 1..]).unwrap_or(rest_trim)
                } else {
                    rest_trim
                };
                if let Some(after) = rest_trim.strip_prefix(':') {
                    let cut = s.len() - after.len();
                    s = s[cut..].trim_start();
                    stripped = true;
                    break;
                }
            }
        }
        if !stripped {
            break;
        }
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_lists() {
        assert_eq!(parse_id_list("<a@x> <b@y>\r\n <c@z>"), vec!["a@x", "b@y", "c@z"]);
        assert_eq!(parse_id_list("<a@x><a@x>"), vec!["a@x"]);
        assert_eq!(parse_id_list("bare@id"), vec!["bare@id"]);
    }

    #[test]
    fn roots() {
        let refs = vec!["root@x".to_string(), "mid@x".to_string()];
        assert_eq!(thread_root(&refs, Some("<mid@x>"), Some("me@x"), "1"), "root@x");
        assert_eq!(thread_root(&[], Some("<parent@x>"), Some("me@x"), "1"), "parent@x");
        assert_eq!(thread_root(&[], None, Some("<me@x>"), "1"), "me@x");
        assert_eq!(thread_root(&[], None, None, "acc:5"), "local:acc:5");
    }

    #[test]
    fn subjects() {
        assert_eq!(normalize_subject("Re: RE: Fwd: Hola"), "Hola");
        assert_eq!(normalize_subject("RV: Informe"), "Informe");
        assert_eq!(normalize_subject("Re[3]: Cena"), "Cena");
        assert_eq!(normalize_subject("Reunión mañana"), "Reunión mañana");
        assert_eq!(normalize_subject("AW: Termin"), "Termin");
    }
}
