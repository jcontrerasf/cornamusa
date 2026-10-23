//! IMAP modified UTF-7 (RFC 3501 §5.1.3), used for mailbox names like "Elementos enviados".

use base64::engine::general_purpose::STANDARD_NO_PAD;
use base64::Engine;

pub fn decode(name: &str) -> String {
    let mut out = String::new();
    let mut rest = name;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        let Some(dash) = after.find('-') else {
            out.push_str(&rest[amp..]);
            return out;
        };
        let chunk = &after[..dash];
        if chunk.is_empty() {
            out.push('&');
        } else {
            match STANDARD_NO_PAD.decode(chunk.replace(',', "/")) {
                Ok(bytes) => {
                    let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes(*c)).collect();
                    out.push_str(&String::from_utf16_lossy(&units));
                }
                Err(_) => out.push_str(&rest[amp..amp + dash + 2]),
            }
        }
        rest = &after[dash + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn decodes() {
        assert_eq!(decode("INBOX"), "INBOX");
        assert_eq!(decode("Elementos env&AO0-ados"), "Elementos envíados");
        assert_eq!(decode("Correo no deseado"), "Correo no deseado");
        assert_eq!(decode("[Gmail]/Enviados"), "[Gmail]/Enviados");
        assert_eq!(decode("A&-B"), "A&B");
        assert_eq!(decode("&ZeVnLIqe-"), "日本語");
    }
}
