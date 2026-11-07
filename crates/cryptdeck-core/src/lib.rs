//! Core logic for Cryptdeck: a backend-agnostic API for mounting and
//! dismounting VeraCrypt-format volumes.
//!
//! The GUI only talks to the [`Backend`] trait. Concrete backends drive an
//! existing, audited tool as a separate process: Cryptdeck never implements
//! volume cryptography itself, and never links VeraCrypt source code.

mod backend;
mod error;
pub mod mock;
mod types;
pub mod veracrypt;

pub use backend::Backend;
pub use error::{Error, Result};
pub use types::{MountRequest, MountedVolume, MAX_PASSWORD_BYTES, MAX_PIM};

use std::sync::Arc;

/// Pick the backend to use.
///
/// `CRYPTDECK_BACKEND=mock` selects an in-memory backend for UI development;
/// otherwise the VeraCrypt CLI backend is used.
pub fn default_backend() -> Arc<dyn Backend> {
    match std::env::var("CRYPTDECK_BACKEND").as_deref() {
        Ok("mock") => Arc::new(mock::MockBackend::default()),
        _ => Arc::new(veracrypt::VeraCryptCli::new()),
    }
}
