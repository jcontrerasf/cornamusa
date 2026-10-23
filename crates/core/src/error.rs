use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IMAP: {0}")]
    Imap(#[from] async_imap::error::Error),
    #[error("SMTP: {0}")]
    Smtp(#[from] lettre::transport::smtp::Error),
    #[error("No se pudo construir el mensaje: {0}")]
    Message(String),
    #[error("Base de datos: {0}")]
    Db(#[from] sqlx::Error),
    #[error("Migración: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("Llavero del sistema: {0}")]
    Keyring(#[from] keyring::Error),
    #[error("HTTP: {0}")]
    Http(#[from] reqwest::Error),
    #[error("E/S: {0}")]
    Io(#[from] std::io::Error),
    #[error("TLS: {0}")]
    Tls(String),
    #[error("Autenticación: {0}")]
    Auth(String),
    #[error("No encontrado: {0}")]
    NotFound(String),
    #[error("Configuración inválida: {0}")]
    Config(String),
    #[error("{0}")]
    Other(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

// Tauri commands return errors to the frontend as plain strings.
impl Serialize for Error {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
