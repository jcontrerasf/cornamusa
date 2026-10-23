//! Credentials live in the OS keychain (Keychain / Credential Manager / Secret Service),
//! never in the database or config files.

use crate::error::{Error, Result};

const SERVICE: &str = "cornamusa";

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
        match self {
            Secrets::Keyring => store(account_id, kind, secret).await,
            Secrets::Memory(m) => {
                m.lock().unwrap().insert(key(account_id, kind), secret.to_string());
                Ok(())
            }
        }
    }

    pub async fn load(&self, account_id: &str, kind: SecretKind) -> Result<String> {
        match self {
            Secrets::Keyring => load(account_id, kind).await,
            Secrets::Memory(m) => m
                .lock()
                .unwrap()
                .get(&key(account_id, kind))
                .cloned()
                .ok_or_else(|| Error::Auth("No hay credenciales guardadas".into())),
        }
    }

    pub async fn delete(&self, account_id: &str) -> Result<()> {
        match self {
            Secrets::Keyring => delete(account_id).await,
            Secrets::Memory(m) => {
                m.lock().unwrap().retain(|k, _| !k.starts_with(&format!("{account_id}:")));
                Ok(())
            }
        }
    }
}

async fn store(account_id: &str, kind: SecretKind, secret: &str) -> Result<()> {
    let (k, s) = (key(account_id, kind), secret.to_string());
    blocking(move || Ok(keyring::Entry::new(SERVICE, &k)?.set_password(&s)?)).await
}

async fn load(account_id: &str, kind: SecretKind) -> Result<String> {
    let k = key(account_id, kind);
    blocking(move || match keyring::Entry::new(SERVICE, &k)?.get_password() {
        Ok(s) => Ok(s),
        Err(keyring::Error::NoEntry) => {
            Err(Error::Auth("No hay credenciales guardadas; vuelve a conectar la cuenta.".into()))
        }
        Err(e) => Err(e.into()),
    })
    .await
}

async fn delete(account_id: &str) -> Result<()> {
    let id = account_id.to_string();
    blocking(move || {
        for kind in [SecretKind::RefreshToken, SecretKind::Password] {
            match keyring::Entry::new(SERVICE, &key(&id, kind))?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    })
    .await
}
