//! What Cryptdeck remembers between runs: the volumes the user has added
//! (`volumes.json`) and their settings (`settings.json`).
//!
//! Only paths are stored, never passwords, PIMs or keyfiles. The files live
//! in the platform's config folder (e.g. `~/.config/cryptdeck` on Linux), or
//! in `$CRYPTDECK_CONFIG_DIR` if that is set.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "volumes.json";
const SETTINGS_FILE: &str = "settings.json";
const VERSION: u32 = 1;

/// User settings. Missing fields (e.g. from an older version) take their
/// defaults.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(default)]
pub struct Settings {
    pub lock_on_sleep: bool,
    pub lock_on_screen_lock: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            lock_on_sleep: true,
            lock_on_screen_lock: false,
        }
    }
}

pub fn load_settings() -> Settings {
    config_dir()
        .and_then(|dir| fs::read(dir.join(SETTINGS_FILE)).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save_settings(settings: &Settings) -> io::Result<()> {
    let dir =
        config_dir().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no config folder"))?;
    write_atomic(
        &dir.join(SETTINGS_FILE),
        &serde_json::to_vec_pretty(settings)?,
    )
}

#[derive(Serialize, Deserialize)]
struct File {
    version: u32,
    volumes: Vec<PathBuf>,
}

fn config_dir() -> Option<PathBuf> {
    std::env::var_os("CRYPTDECK_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::config_dir().map(|d| d.join("cryptdeck")))
}

/// Load the saved volumes. A missing or unreadable file means an empty list:
/// losing the list is an inconvenience, not a reason to refuse to start.
pub fn load() -> Vec<PathBuf> {
    config_dir()
        .and_then(|dir| load_from(&dir.join(FILE_NAME)).ok())
        .unwrap_or_default()
}

pub fn save(volumes: &[PathBuf]) -> io::Result<()> {
    let dir =
        config_dir().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no config folder"))?;
    save_to(&dir.join(FILE_NAME), volumes)
}

fn load_from(path: &Path) -> io::Result<Vec<PathBuf>> {
    let file: File = serde_json::from_slice(&fs::read(path)?)?;
    Ok(file.volumes)
}

fn save_to(path: &Path, volumes: &[PathBuf]) -> io::Result<()> {
    let json = serde_json::to_vec_pretty(&File {
        version: VERSION,
        volumes: volumes.to_vec(),
    })?;
    write_atomic(path, &json)
}

/// Write via a temporary file and rename, so a crash mid-write can't leave
/// a truncated file behind.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let dir = std::env::temp_dir().join(format!("cryptdeck-store-{}", std::process::id()));
        let path = dir.join(FILE_NAME);
        let volumes = vec![PathBuf::from("/a/one.hc"), PathBuf::from("/b/two words.hc")];

        save_to(&path, &volumes).unwrap();
        assert_eq!(load_from(&path).unwrap(), volumes);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn settings_default_and_fill_missing_fields() {
        assert!(Settings::default().lock_on_sleep);
        assert!(!Settings::default().lock_on_screen_lock);
        let partial: Settings = serde_json::from_str(r#"{"lock_on_screen_lock": true}"#).unwrap();
        assert_eq!(
            partial,
            Settings {
                lock_on_sleep: true,
                lock_on_screen_lock: true
            }
        );
    }

    #[test]
    fn missing_file_is_an_error_not_a_panic() {
        assert!(load_from(Path::new("/definitely/not/here.json")).is_err());
    }
}
