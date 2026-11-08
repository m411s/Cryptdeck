//! Core logic for Cryptdeck: a backend-agnostic API for mounting and
//! dismounting VeraCrypt-format volumes.
//!
//! The GUI only talks to the [`Backend`] trait. Concrete backends drive an
//! existing, audited tool as a separate process: Cryptdeck never implements
//! volume cryptography itself, and never links VeraCrypt source code.

rust_i18n::i18n!("locales", fallback = "en");

mod backend;
mod error;
pub mod mock;
mod types;
#[cfg(target_os = "linux")]
pub mod udisks;
pub mod veracrypt;
pub mod windows;

pub use backend::Backend;
pub use error::{Error, Result};
pub use types::{MountRequest, MountedVolume, MAX_PASSWORD_BYTES, MAX_PIM};

use std::sync::Arc;

/// Pick the backend to use.
///
/// On Linux, the system's udisks2 service is preferred when it's new enough:
/// it needs no VeraCrypt install and mounts volumes as the user. Otherwise
/// the VeraCrypt CLI is used. On Windows, VeraCrypt.exe and its driver are
/// used. `CRYPTDECK_BACKEND` forces a choice: `udisks`,
/// `veracrypt`, or `mock` (an in-memory backend for UI development).
pub fn default_backend() -> Arc<dyn Backend> {
    match std::env::var("CRYPTDECK_BACKEND").as_deref() {
        Ok("mock") => return Arc::new(mock::MockBackend::default()),
        #[cfg(not(windows))]
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
    #[cfg(windows)]
    return Arc::new(windows::VeraCryptWin::new());
    #[cfg(not(windows))]
    Arc::new(veracrypt::VeraCryptCli::new())
}

#[cfg(test)]
mod translation_tests {
    /// Every `t!("key")` in the source must exist in English and German;
    /// a missing key would show up in the UI as the raw key.
    #[test]
    fn every_key_is_translated() {
        let sources = [
            include_str!("error.rs"),
            include_str!("types.rs"),
            include_str!("udisks.rs"),
            include_str!("windows.rs"),
        ];
        let mut keys: Vec<&str> = Vec::new();
        for src in sources {
            // Skip this test module, whose comments mention `t!`.
            let src = src.split("mod translation_tests").next().unwrap_or(src);
            for (i, _) in src.match_indices("t!(\"") {
                // A standalone `t!(`, not the end of e.g. `format!(`.
                let before = src[..i].chars().next_back().unwrap_or(' ');
                if before.is_alphanumeric() || before == '_' {
                    continue;
                }
                let rest = &src[i + 4..];
                keys.extend(rest.split('"').next());
            }
        }
        keys.sort();
        keys.dedup();
        assert!(keys.len() > 20, "found only {} keys", keys.len());
        for locale in ["en", "de"] {
            for key in &keys {
                let text = rust_i18n::t!(*key, locale = locale);
                assert_ne!(&*text, *key, "{key} has no {locale} translation");
            }
        }
    }
}
