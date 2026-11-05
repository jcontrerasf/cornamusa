//! Credentials live in the OS keychain (Keychain / Credential Manager / Secret Service),
//! never in the database or config files.
//!
//! Windows' Credential Manager caps each credential at 2560 bytes (UTF-16), and Microsoft
//! work-account refresh tokens can exceed that, so long secrets are split across several
//! entries: `{id}:{kind}#1..#N`, with the main `{id}:{kind}` entry holding a marker with N.
//! Short secrets are stored as a single entry, exactly as before.

use crate::error::{Error, Result};

const SERVICE: &str = "cornamusa";
/// Max chars per keychain entry (2000 bytes in UTF-16, under Windows' 2560-byte limit).
const CHUNK: usize = 1000;
const MARKER: &str = "\u{1}cornamusa-chunks:";

#[derive(Debug, Clone, Copy)]
pub enum SecretKind {
    RefreshToken,
    Password,
}

impl SecretKind {
    fn suffix(self) -> &'static str {
        match self {
            SecretKind::RefreshToken => "oauth-refresh",
            SecretKind::Password => "password",
        }
    }
}

fn key(account_id: &str, kind: SecretKind) -> String {
    format!("{account_id}:{}", kind.suffix())
}

fn part_key(main: &str, n: usize) -> String {
    format!("{main}#{n}")
}

fn chunk_count(main_value: &str) -> Option<usize> {
    main_value.strip_prefix(MARKER)?.parse().ok()
}

fn missing() -> Error {
    Error::Auth("No hay credenciales guardadas; vuelve a conectar la cuenta.".into())
}

// keyring is blocking (and the Secret Service backend runs its own executor), so keep it off
// the async runtime threads.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(f).await.map_err(|e| Error::Other(e.to_string()))?
}

/// Where credentials are kept: the OS keychain in the app, memory in tests.
pub enum Secrets {
    Keyring,
    Memory(std::sync::Mutex<std::collections::HashMap<String, String>>),
}

impl Secrets {
    pub fn memory() -> Self {
        Secrets::Memory(Default::default())
    }

    pub async fn store(&self, account_id: &str, kind: SecretKind, secret: &str) -> Result<()> {
        let main = key(account_id, kind);
        let chars: Vec<char> = secret.chars().collect();
        let first_stale = if chars.len() <= CHUNK {
            self.raw_set(&main, secret).await?;
            1
        } else {
            let parts: Vec<String> = chars.chunks(CHUNK).map(|c| c.iter().collect()).collect();
            for (i, p) in parts.iter().enumerate() {
                self.raw_set(&part_key(&main, i + 1), p).await?;
            }
            // The main entry goes last: it is what makes the new parts current.
            self.raw_set(&main, &format!("{MARKER}{}", parts.len())).await?;
            parts.len() + 1
        };
        // Drop parts left over from a longer previous value.
        let mut n = first_stale;
        while self.raw_delete(&part_key(&main, n)).await? {
            n += 1;
        }
        Ok(())
    }

    pub async fn load(&self, account_id: &str, kind: SecretKind) -> Result<String> {
        let main = key(account_id, kind);
        let value = self.raw_get(&main).await?.ok_or_else(missing)?;
        let Some(count) = chunk_count(&value) else { return Ok(value) };
        let mut out = String::new();
        for n in 1..=count {
            out.push_str(&self.raw_get(&part_key(&main, n)).await?.ok_or_else(missing)?);
        }
        Ok(out)
    }

    pub async fn delete(&self, account_id: &str) -> Result<()> {
        for kind in [SecretKind::RefreshToken, SecretKind::Password] {
            let main = key(account_id, kind);
            let count = self.raw_get(&main).await?.as_deref().and_then(chunk_count).unwrap_or(0);
            for n in 1..=count {
                self.raw_delete(&part_key(&main, n)).await?;
            }
            self.raw_delete(&main).await?;
        }
        Ok(())
    }

    // ---------- raw single-entry operations ----------

    async fn raw_set(&self, k: &str, v: &str) -> Result<()> {
        match self {
            Secrets::Keyring => {
                let (k, v) = (k.to_string(), v.to_string());
                blocking(move || Ok(keyring::Entry::new(SERVICE, &k)?.set_password(&v)?)).await
            }
            Secrets::Memory(m) => {
                m.lock().unwrap().insert(k.to_string(), v.to_string());
                Ok(())
            }
        }
    }

    async fn raw_get(&self, k: &str) -> Result<Option<String>> {
        match self {
            Secrets::Keyring => {
                let k = k.to_string();
                blocking(move || match keyring::Entry::new(SERVICE, &k)?.get_password() {
                    Ok(s) => Ok(Some(s)),
                    Err(keyring::Error::NoEntry) => Ok(None),
                    Err(e) => Err(e.into()),
                })
                .await
            }
            Secrets::Memory(m) => Ok(m.lock().unwrap().get(k).cloned()),
        }
    }

    /// Returns whether an entry existed.
    async fn raw_delete(&self, k: &str) -> Result<bool> {
        match self {
            Secrets::Keyring => {
                let k = k.to_string();
                blocking(move || match keyring::Entry::new(SERVICE, &k)?.delete_credential() {
                    Ok(()) => Ok(true),
                    Err(keyring::Error::NoEntry) => Ok(false),
                    Err(e) => Err(e.into()),
                })
                .await
            }
            Secrets::Memory(m) => Ok(m.lock().unwrap().remove(k).is_some()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(s: &Secrets) -> Vec<String> {
        let Secrets::Memory(m) = s else { unreachable!() };
        let mut v: Vec<String> = m.lock().unwrap().keys().cloned().collect();
        v.sort();
        v
    }

    fn token(len: usize) -> String {
        (0..len).map(|i| char::from(b'a' + (i % 26) as u8)).collect()
    }

    #[tokio::test]
    async fn short_secret_is_a_single_entry() {
        let s = Secrets::memory();
        s.store("a", SecretKind::Password, "hunter2").await.unwrap();
        assert_eq!(s.load("a", SecretKind::Password).await.unwrap(), "hunter2");
        assert_eq!(keys(&s), ["a:password"]);
    }

    #[tokio::test]
    async fn exactly_chunk_is_not_split() {
        let s = Secrets::memory();
        let t = token(CHUNK);
        s.store("a", SecretKind::RefreshToken, &t).await.unwrap();
        assert_eq!(s.load("a", SecretKind::RefreshToken).await.unwrap(), t);
        assert_eq!(keys(&s).len(), 1);
    }

    #[tokio::test]
    async fn long_secret_round_trips_in_parts() {
        let s = Secrets::memory();
        let t = token(5000) + "ñé"; // multibyte chars must not be cut
        s.store("a", SecretKind::RefreshToken, &t).await.unwrap();
        assert_eq!(s.load("a", SecretKind::RefreshToken).await.unwrap(), t);
        assert_eq!(keys(&s).len(), 7); // main + 6 parts
    }

    #[tokio::test]
    async fn shrinking_and_growing_leave_no_orphans() {
        let s = Secrets::memory();
        s.store("a", SecretKind::RefreshToken, &token(4500)).await.unwrap();
        s.store("a", SecretKind::RefreshToken, &token(1500)).await.unwrap();
        assert_eq!(keys(&s).len(), 3);
        assert_eq!(s.load("a", SecretKind::RefreshToken).await.unwrap(), token(1500));
        s.store("a", SecretKind::RefreshToken, "short").await.unwrap();
        assert_eq!(keys(&s), ["a:oauth-refresh"]);
        s.store("a", SecretKind::RefreshToken, &token(2500)).await.unwrap();
        assert_eq!(s.load("a", SecretKind::RefreshToken).await.unwrap(), token(2500));
    }

    #[tokio::test]
    async fn delete_removes_parts() {
        let s = Secrets::memory();
        s.store("a", SecretKind::RefreshToken, &token(3000)).await.unwrap();
        s.store("a", SecretKind::Password, "pw").await.unwrap();
        s.store("b", SecretKind::Password, "other").await.unwrap();
        s.delete("a").await.unwrap();
        assert_eq!(keys(&s), ["b:password"]);
    }

    #[tokio::test]
    async fn missing_part_is_an_auth_error() {
        let s = Secrets::memory();
        s.store("a", SecretKind::RefreshToken, &token(3000)).await.unwrap();
        s.raw_delete("a:oauth-refresh#2").await.unwrap();
        assert!(matches!(s.load("a", SecretKind::RefreshToken).await, Err(Error::Auth(_))));
        assert!(matches!(s.load("x", SecretKind::Password).await, Err(Error::Auth(_))));
    }

    #[tokio::test]
    async fn legacy_long_single_entry_still_loads() {
        let s = Secrets::memory();
        let t = token(3000);
        s.raw_set("a:oauth-refresh", &t).await.unwrap();
        assert_eq!(s.load("a", SecretKind::RefreshToken).await.unwrap(), t);
    }
}
