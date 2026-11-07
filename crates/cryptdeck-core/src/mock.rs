//! In-memory backend for developing and testing the UI without VeraCrypt or
//! root. Select it with `CRYPTDECK_BACKEND=mock`. Any file is
//! accepted as a volume, and the password must be `test`.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use secrecy::ExposeSecret;

use crate::{Backend, Error, MountRequest, MountedVolume, Result};

#[derive(Debug, Default)]
pub struct MockBackend {
    mounted: Mutex<Vec<MountedVolume>>,
}

impl Backend for MockBackend {
    fn name(&self) -> &str {
        "Mock (demo)"
    }

    fn check(&self) -> Result<String> {
        Ok("mock backend, password is \"test\"".into())
    }

    fn list(&self) -> Result<Vec<MountedVolume>> {
        Ok(self.mounted.lock().unwrap().clone())
    }

    fn mount(&self, req: &MountRequest) -> Result<()> {
        req.validate()?;
        std::thread::sleep(Duration::from_millis(400));
        if req.password.expose_secret() != "test" {
            return Err(Error::CommandFailed(
                "Incorrect password or not a VeraCrypt volume.".into(),
            ));
        }
        let mut mounted = self.mounted.lock().unwrap();
        if mounted.iter().any(|v| v.volume == req.volume) {
            return Err(Error::CommandFailed(
                "The volume is already mounted.".into(),
            ));
        }
        let slot = (1..)
            .find(|n| !mounted.iter().any(|v| v.slot == *n))
            .unwrap();
        mounted.push(MountedVolume {
            slot,
            volume: req.volume.clone(),
            virtual_device: PathBuf::from(format!("/dev/mapper/veracrypt{slot}")),
            mount_point: Some(
                req.mount_point
                    .clone()
                    .unwrap_or_else(|| PathBuf::from(format!("/media/veracrypt{slot}"))),
            ),
        });
        mounted.sort_by_key(|v| v.slot);
        Ok(())
    }

    fn dismount(&self, volume: &MountedVolume) -> Result<()> {
        self.mounted
            .lock()
            .unwrap()
            .retain(|v| v.slot != volume.slot);
        Ok(())
    }

    fn dismount_all(&self) -> Result<()> {
        self.mounted.lock().unwrap().clear();
        Ok(())
    }
}
