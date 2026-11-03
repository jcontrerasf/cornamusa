//! Cornamusa core: protocols, storage and mail logic, independent of the UI shell.

pub mod attachments;
pub mod error;
pub mod imap;
pub mod mime;
pub mod model;
pub mod oauth;
pub mod providers;
pub mod quote_strip;
pub mod sanitize;
pub mod smtp;
pub mod secrets;
pub mod service;
pub mod store;
pub mod threading;
pub mod utf7;

pub use error::{Error, Result};
