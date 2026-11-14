use crate::{MountRequest, MountedVolume, Result};

/// Something that can mount and dismount VeraCrypt-format volumes.
///
/// All methods are blocking; callers with a UI should run them off the UI
/// thread.
pub trait Backend: Send + Sync {
    /// Short human-readable name, e.g. "VeraCrypt CLI".
    fn name(&self) -> &str;

    /// Check that the backend is usable and return a version string.
    fn check(&self) -> Result<String>;

    /// List currently mounted volumes.
    fn list(&self) -> Result<Vec<MountedVolume>>;

    /// Whether [`MountRequest::mount_point`] is honoured. Some backends
    /// always choose the mount point themselves.
    fn supports_mount_point(&self) -> bool {
        true
    }

    /// Make the one-time system change that [`crate::Error::SetupNeeded`]
    /// asks for. It may show the desktop's admin password dialog.
    fn setup(&self) -> Result<()> {
        Ok(())
    }

    /// Mount a volume.
    fn mount(&self, request: &MountRequest) -> Result<()>;

    /// Dismount one volume.
    fn dismount(&self, volume: &MountedVolume) -> Result<()>;

    /// Dismount every volume this backend manages.
    fn dismount_all(&self) -> Result<()>;
}
