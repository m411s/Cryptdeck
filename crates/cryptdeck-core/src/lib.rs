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
#[cfg(target_os = "linux")]
pub mod udisks;
pub mod veracrypt;

pub use backend::Backend;
pub use error::{Error, Result};
pub use types::{MountRequest, MountedVolume, MAX_PASSWORD_BYTES, MAX_PIM};

use std::sync::Arc;

/// Pick the backend to use.
///
/// On Linux, the system's udisks2 service is preferred when it's new enough:
/// it needs no VeraCrypt install and mounts volumes as the user. Otherwise
/// the VeraCrypt CLI is used. `CRYPTDECK_BACKEND` forces a choice: `udisks`,
/// `veracrypt`, or `mock` (an in-memory backend for UI development).
pub fn default_backend() -> Arc<dyn Backend> {
    match std::env::var("CRYPTDECK_BACKEND").as_deref() {
        Ok("mock") => return Arc::new(mock::MockBackend::default()),
        Ok("veracrypt") => return Arc::new(veracrypt::VeraCryptCli::new()),
        #[cfg(target_os = "linux")]
        Ok("udisks") => match udisks::Udisks::connect() {
            Ok(u) => return Arc::new(u),
            Err(e) => eprintln!("cryptdeck: udisks unavailable ({e}); using VeraCrypt"),
        },
        _ =>
        {
            #[cfg(target_os = "linux")]
            if let Ok(u) = udisks::Udisks::connect() {
                return Arc::new(u);
            }
        }
    }
    Arc::new(veracrypt::VeraCryptCli::new())
}
