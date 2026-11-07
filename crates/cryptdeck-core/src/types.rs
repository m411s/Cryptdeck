use std::path::PathBuf;

use secrecy::{ExposeSecret, SecretString};

use crate::{Error, Result};

/// Largest PIM value VeraCrypt accepts.
pub const MAX_PIM: u32 = 2_147_468;

/// Longest password VeraCrypt accepts, in bytes of UTF-8.
pub const MAX_PASSWORD_BYTES: usize = 128;

/// Everything needed to mount a volume.
///
/// The password is held in a [`SecretString`], which is zeroized on drop and
/// redacted from `Debug` output.
#[derive(Debug)]
pub struct MountRequest {
    /// Container file or device to mount.
    pub volume: PathBuf,
    /// Where to mount it. `None` lets the backend choose.
    pub mount_point: Option<PathBuf>,
    pub password: SecretString,
    /// Personal Iterations Multiplier. `None` means the default.
    pub pim: Option<u32>,
    pub keyfiles: Vec<PathBuf>,
    pub read_only: bool,
}

impl MountRequest {
    /// Check the request for problems that can be caught before running anything.
    pub fn validate(&self) -> Result<()> {
        if self.volume.as_os_str().is_empty() {
            return Err(Error::InvalidInput("Choose a volume to mount".into()));
        }
        if !self.volume.exists() {
            return Err(Error::InvalidInput(format!(
                "Volume not found: {}",
                self.volume.display()
            )));
        }
        if self.volume.is_dir() {
            return Err(Error::InvalidInput(format!(
                "The volume must be a file or a device, not a folder: {}",
                self.volume.display()
            )));
        }
        if let Some(mp) = &self.mount_point {
            if !mp.is_dir() {
                return Err(Error::InvalidInput(format!(
                    "Mount point is not a directory: {}",
                    mp.display()
                )));
            }
        }
        let password = self.password.expose_secret();
        if password.is_empty() && self.keyfiles.is_empty() {
            return Err(Error::InvalidInput(
                "Enter a password or add at least one keyfile".into(),
            ));
        }
        if password.len() > MAX_PASSWORD_BYTES {
            return Err(Error::InvalidInput(format!(
                "VeraCrypt passwords can be at most {MAX_PASSWORD_BYTES} bytes long"
            )));
        }
        if password.contains(['\n', '\r']) {
            return Err(Error::InvalidInput(
                "Passwords containing line breaks are not supported".into(),
            ));
        }
        if let Some(pim) = self.pim {
            if pim > MAX_PIM {
                return Err(Error::InvalidInput(format!(
                    "PIM must be at most {MAX_PIM}"
                )));
            }
        }
        for kf in &self.keyfiles {
            if !kf.exists() {
                return Err(Error::InvalidInput(format!(
                    "Keyfile not found: {}",
                    kf.display()
                )));
            }
            // VeraCrypt takes keyfiles as a comma-separated list.
            if kf.to_string_lossy().contains(',') {
                return Err(Error::InvalidInput(format!(
                    "Keyfile paths containing commas are not supported: {}",
                    kf.display()
                )));
            }
        }
        Ok(())
    }
}

/// A volume that is currently mounted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountedVolume {
    /// Slot number (VeraCrypt's "volume #").
    pub slot: u32,
    /// The container file or device.
    pub volume: PathBuf,
    /// The decrypted virtual device, e.g. `/dev/mapper/veracrypt1`.
    pub virtual_device: PathBuf,
    /// Where its filesystem is mounted, if anywhere.
    pub mount_point: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A request for a file that is guaranteed to exist.
    fn request(password: &str) -> MountRequest {
        MountRequest {
            volume: std::env::current_exe().unwrap(),
            mount_point: None,
            password: SecretString::from(password),
            pim: None,
            keyfiles: vec![],
            read_only: false,
        }
    }

    fn error_of(req: &MountRequest) -> String {
        req.validate().unwrap_err().to_string()
    }

    #[test]
    fn accepts_valid_request() {
        request("correct horse").validate().unwrap();
    }

    #[test]
    fn rejects_missing_password_and_keyfiles() {
        assert!(error_of(&request("")).contains("password or add"));
    }

    #[test]
    fn rejects_overlong_password() {
        request(&"a".repeat(MAX_PASSWORD_BYTES)).validate().unwrap();
        assert!(error_of(&request(&"a".repeat(MAX_PASSWORD_BYTES + 1))).contains("at most"));
        // The limit is in bytes, not characters.
        assert!(error_of(&request(&"é".repeat(65))).contains("at most"));
    }

    #[test]
    fn rejects_line_breaks() {
        assert!(error_of(&request("a\nb")).contains("line breaks"));
    }

    #[test]
    fn rejects_folder_as_volume() {
        let mut req = request("pw");
        req.volume = std::env::temp_dir();
        assert!(error_of(&req).contains("not a folder"));
    }

    #[test]
    fn rejects_out_of_range_pim() {
        let mut req = request("pw");
        req.pim = Some(MAX_PIM + 1);
        assert!(error_of(&req).contains("PIM"));
    }
}
