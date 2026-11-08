//! Backend that uses udisks2, the system service behind GNOME Disks and
//! KDE's file manager, to open VeraCrypt volumes (Linux only).
//!
//! udisks opens VeraCrypt volumes with the system's `cryptsetup`, so
//! VeraCrypt doesn't need to be installed. It mounts them as the user
//! (under `/run/media/<user>`), so files are writable without root. For a
//! user's own container files, desktop polkit rules usually allow this
//! without an admin password; when one is needed, the desktop asks for it.
//! Cryptdeck itself never runs anything as root here.
//!
//! The sequence for a container file is: open the file and hand its file
//! descriptor to `Manager.LoopSetup`, then `Encrypted.Unlock` the loop
//! device, then `Filesystem.Mount` the unlocked device. Locking reverses it.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use secrecy::ExposeSecret;
use zbus::blocking::{Connection, Proxy};
use zbus::proxy::MethodFlags;
use zbus::zvariant::{Fd, OwnedObjectPath, OwnedValue, Value};
use zeroize::Zeroizing;

use crate::{Backend, Error, MountRequest, MountedVolume, Result};

const SERVICE: &str = "org.freedesktop.UDisks2";
const ROOT: &str = "/org/freedesktop/UDisks2";
const MANAGER: &str = "/org/freedesktop/UDisks2/Manager";
const IFACE_MANAGER: &str = "org.freedesktop.UDisks2.Manager";
const IFACE_BLOCK: &str = "org.freedesktop.UDisks2.Block";
const IFACE_LOOP: &str = "org.freedesktop.UDisks2.Loop";
const IFACE_ENCRYPTED: &str = "org.freedesktop.UDisks2.Encrypted";
const IFACE_FILESYSTEM: &str = "org.freedesktop.UDisks2.Filesystem";

/// udisks gained VeraCrypt (TCRYPT) unlocking with PIM and keyfiles in 2.9.
const MIN_VERSION: (u32, u32) = (2, 9);

/// How long to wait for udisks to finish probing a new device.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

type Props = HashMap<String, OwnedValue>;
type Interfaces = HashMap<String, Props>;
type Objects = HashMap<String, Interfaces>;

pub struct Udisks {
    conn: Connection,
    version: String,
}

impl Udisks {
    /// Connect to udisks and check that it's new enough. `Err` means the
    /// caller should fall back to another backend.
    pub fn connect() -> Result<Self> {
        // Tests point this at a stand-in service on the session bus.
        let conn = match std::env::var("CRYPTDECK_UDISKS_BUS").as_deref() {
            Ok("session") => Connection::session(),
            _ => Connection::system(),
        }
        .map_err(dbus_error)?;
        let manager = proxy(&conn, MANAGER, IFACE_MANAGER)?;
        let version: String = manager.get_property("Version").map_err(dbus_error)?;
        if parse_version(&version) < Some(MIN_VERSION) {
            return Err(Error::CommandFailed(format!(
                "udisks {version} is too old to open VeraCrypt volumes (2.9 or newer needed)"
            )));
        }
        Ok(Self { conn, version })
    }

    fn objects(&self) -> Result<Objects> {
        let objects: HashMap<OwnedObjectPath, Interfaces> =
            proxy(&self.conn, ROOT, "org.freedesktop.DBus.ObjectManager")?
                .call("GetManagedObjects", &())
                .map_err(dbus_error)?;
        Ok(objects
            .into_iter()
            .map(|(path, ifaces)| (path.to_string(), ifaces))
            .collect())
    }

    /// Call a method, letting the desktop ask for a password if polkit
    /// requires one.
    fn call<B, R>(&self, path: &str, iface: &str, method: &str, body: &B) -> Result<R>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
        R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
    {
        let p = proxy(&self.conn, path, iface)?;
        let reply = p
            .call_with_flags::<_, B, R>(method, MethodFlags::AllowInteractiveAuth.into(), body)
            .map_err(dbus_error)?;
        reply.ok_or_else(|| Error::CommandFailed(format!("udisks gave no reply to {method}")))
    }

    /// Wait until `path` has `iface`; udisks adds interfaces once it has
    /// probed a new device.
    fn wait_for(&self, path: &str, iface: &str) -> Result<Interfaces> {
        let deadline = Instant::now() + PROBE_TIMEOUT;
        loop {
            if let Some(ifaces) = self.objects()?.remove(path) {
                if ifaces.contains_key(iface) {
                    return Ok(ifaces);
                }
            }
            if Instant::now() >= deadline {
                return Err(Error::CommandFailed(format!(
                    "udisks didn't finish preparing {path} (no {iface})"
                )));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Find the udisks object for a block device such as `/dev/sdb1`.
    fn find_block(&self, device: &Path) -> Result<String> {
        let device = device
            .canonicalize()
            .unwrap_or_else(|_| device.to_path_buf());
        self.objects()?
            .into_iter()
            .find(|(_, ifaces)| {
                ifaces
                    .get(IFACE_BLOCK)
                    .and_then(|b| path_prop(b, "Device"))
                    .is_some_and(|d| d == device)
            })
            .map(|(path, _)| path)
            .ok_or_else(|| {
                Error::CommandFailed(format!("udisks doesn't know {}", device.display()))
            })
    }

    /// Undo a half-finished unlock, ignoring errors: we're already failing.
    fn clean_up(&self, crypto: &str, loop_dev: Option<&str>, unlocked: bool) {
        if unlocked {
            let _: Result<()> = self.call(crypto, IFACE_ENCRYPTED, "Lock", &(no_options(),));
        }
        if let Some(l) = loop_dev {
            let _: Result<()> = self.call(l, IFACE_LOOP, "Delete", &(no_options(),));
        }
    }

    fn dismount_one(&self, objects: &Objects, cleartext: &str) -> Result<()> {
        let ifaces = objects
            .get(cleartext)
            .ok_or_else(|| Error::CommandFailed("The volume is no longer unlocked".into()))?;
        if ifaces
            .get(IFACE_FILESYSTEM)
            .is_some_and(|fs| !mount_points(fs).is_empty())
        {
            self.call::<_, ()>(cleartext, IFACE_FILESYSTEM, "Unmount", &(no_options(),))?;
        }
        let backing = ifaces
            .get(IFACE_BLOCK)
            .and_then(|b| object_prop(b, "CryptoBackingDevice"))
            .ok_or_else(|| Error::CommandFailed("Couldn't find the encrypted device".into()))?;
        self.call::<_, ()>(&backing, IFACE_ENCRYPTED, "Lock", &(no_options(),))?;
        if objects
            .get(backing.as_str())
            .is_some_and(|b| b.contains_key(IFACE_LOOP))
        {
            self.call::<_, ()>(&backing, IFACE_LOOP, "Delete", &(no_options(),))?;
        }
        Ok(())
    }
}

impl Backend for Udisks {
    fn name(&self) -> &str {
        "System (udisks2)"
    }

    fn check(&self) -> Result<String> {
        Ok(format!(
            "Using your system's disk service (udisks {})",
            self.version
        ))
    }

    fn supports_mount_point(&self) -> bool {
        false
    }

    fn list(&self) -> Result<Vec<MountedVolume>> {
        Ok(unlocked_volumes(&self.objects()?)
            .into_iter()
            .map(|(_, v)| v)
            .collect())
    }

    fn mount(&self, request: &MountRequest) -> Result<()> {
        request.validate()?;
        if request.mount_point.is_some() {
            return Err(Error::InvalidInput(
                "Your system chooses where volumes appear, so “Unlock to folder” isn't available"
                    .into(),
            ));
        }
        let volume = request.volume.canonicalize()?;
        if self.list()?.iter().any(|v| v.volume == volume) {
            return Err(Error::AlreadyMounted);
        }

        // Keyfiles are passed by content. Read them before touching any
        // device, so a bad keyfile doesn't leave a loop device behind.
        let mut keyfiles = Vec::new();
        for kf in &request.keyfiles {
            if kf.is_dir() {
                return Err(Error::InvalidInput(format!(
                    "Keyfile folders aren't supported here; add the files inside “{}” instead",
                    kf.display()
                )));
            }
            keyfiles.push(Zeroizing::new(std::fs::read(kf)?));
        }

        // A container file needs a loop device; a partition is used as is.
        let is_device = std::fs::metadata(&volume)?.file_type().is_block_device();
        let (crypto, loop_dev) = if is_device {
            (self.find_block(&volume)?, None)
        } else {
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(!request.read_only)
                .open(&volume)?;
            let mut options = HashMap::new();
            options.insert("read-only", Value::from(request.read_only));
            options.insert("no-part-scan", Value::from(true));
            let path: OwnedObjectPath = self.call(
                MANAGER,
                IFACE_MANAGER,
                "LoopSetup",
                &(Fd::from(&file), options),
            )?;
            (path.to_string(), Some(path.to_string()))
        };

        if let Err(e) = self.wait_for(&crypto, IFACE_ENCRYPTED) {
            self.clean_up(&crypto, loop_dev.as_deref(), false);
            return Err(e);
        }

        let mut options: HashMap<&str, Value> = HashMap::new();
        if let Some(pim) = request.pim {
            options.insert("pim", Value::from(pim));
        }
        if !keyfiles.is_empty() {
            let contents: Vec<Vec<u8>> = keyfiles.iter().map(|k| k.to_vec()).collect();
            options.insert("keyfiles", Value::from(contents));
        }
        options.insert("read-only", Value::from(request.read_only));
        let password = request.password.expose_secret();
        let cleartext: OwnedObjectPath =
            match self.call(&crypto, IFACE_ENCRYPTED, "Unlock", &(password, options)) {
                Ok(p) => p,
                Err(e) => {
                    self.clean_up(&crypto, loop_dev.as_deref(), false);
                    return Err(unlock_error(e));
                }
            };

        let mounted = self
            .wait_for(cleartext.as_str(), IFACE_FILESYSTEM)
            .and_then(|_| {
                let mut options = HashMap::new();
                if request.read_only {
                    options.insert("options", Value::from("ro"));
                }
                self.call::<_, String>(cleartext.as_str(), IFACE_FILESYSTEM, "Mount", &(options,))
            });
        if let Err(e) = mounted {
            self.clean_up(&crypto, loop_dev.as_deref(), true);
            return Err(e);
        }
        Ok(())
    }

    fn dismount(&self, volume: &MountedVolume) -> Result<()> {
        let objects = self.objects()?;
        let cleartext = unlocked_volumes(&objects)
            .into_iter()
            .find(|(_, v)| v.virtual_device == volume.virtual_device)
            .map(|(path, _)| path)
            .ok_or_else(|| Error::CommandFailed("The volume is no longer unlocked".into()))?;
        self.dismount_one(&objects, &cleartext)
    }

    fn dismount_all(&self) -> Result<()> {
        let objects = self.objects()?;
        let mut first_error = None;
        for (cleartext, _) in unlocked_volumes(&objects) {
            if let Err(e) = self.dismount_one(&objects, &cleartext) {
                first_error.get_or_insert(e);
            }
        }
        first_error.map_or(Ok(()), Err)
    }
}

/// Unlocked VeraCrypt volumes, with the object path of each unlocked
/// (cleartext) device.
///
/// LUKS and BitLocker devices are skipped: udisks identifies those, while
/// VeraCrypt volumes have no recognisable header, so their encrypted
/// device's `IdType` is empty.
fn unlocked_volumes(objects: &Objects) -> Vec<(String, MountedVolume)> {
    let mut found: Vec<(String, MountedVolume)> = objects
        .iter()
        .filter_map(|(path, ifaces)| {
            let block = ifaces.get(IFACE_BLOCK)?;
            let backing_path = object_prop(block, "CryptoBackingDevice")?;
            let backing = objects.get(backing_path.as_str())?;
            let backing_block = backing.get(IFACE_BLOCK)?;
            if !string_prop(backing_block, "IdType")
                .unwrap_or_default()
                .is_empty()
            {
                return None;
            }
            let volume = backing
                .get(IFACE_LOOP)
                .and_then(|l| path_prop(l, "BackingFile"))
                .or_else(|| path_prop(backing_block, "Device"))?;
            Some((
                path.clone(),
                MountedVolume {
                    slot: 0,
                    volume,
                    virtual_device: path_prop(block, "Device")?,
                    mount_point: ifaces
                        .get(IFACE_FILESYSTEM)
                        .and_then(|fs| mount_points(fs).into_iter().next()),
                },
            ))
        })
        .collect();
    found.sort_by(|a, b| a.1.virtual_device.cmp(&b.1.virtual_device));
    for (i, (_, v)) in found.iter_mut().enumerate() {
        v.slot = i as u32 + 1;
    }
    found
}

fn proxy<'a>(conn: &Connection, path: &'a str, iface: &'a str) -> Result<Proxy<'a>> {
    Proxy::new(conn, SERVICE, path, iface).map_err(dbus_error)
}

fn no_options() -> HashMap<&'static str, Value<'static>> {
    HashMap::new()
}

fn dbus_error(e: zbus::Error) -> Error {
    let message = match &e {
        zbus::Error::MethodError(name, Some(detail), _) => {
            if name.as_str().contains("NotAuthorized") {
                return Error::AuthCancelled;
            }
            detail.clone()
        }
        zbus::Error::MethodError(name, None, _) => name.to_string(),
        other => other.to_string(),
    };
    crate::error::classify(message)
}

/// cryptsetup reports a wrong TCRYPT password as "Operation not permitted"
/// (EPERM) or as a missing header, rather than mentioning the password.
fn unlock_error(e: Error) -> Error {
    match e {
        Error::CommandFailed(msg)
            if msg.contains("Operation not permitted")
                || msg.contains("No device header")
                || msg.contains("passphrase") =>
        {
            Error::WrongPassword
        }
        other => other,
    }
}

fn parse_version(version: &str) -> Option<(u32, u32)> {
    let mut parts = version.split('.').map(|p| p.parse::<u32>().ok());
    Some((parts.next()??, parts.next()??))
}

/// A NUL-terminated byte string property (`ay`), such as a device path.
fn path_prop(props: &Props, key: &str) -> Option<PathBuf> {
    bytes_to_path(&bytes(props.get(key)?)?)
}

fn mount_points(fs: &Props) -> Vec<PathBuf> {
    match fs.get("MountPoints").map(|v| &**v) {
        Some(Value::Array(points)) => points
            .iter()
            .filter_map(|p| bytes_to_path(&bytes(p)?))
            .collect(),
        _ => Vec::new(),
    }
}

fn bytes(value: &Value) -> Option<Vec<u8>> {
    match value {
        Value::Array(a) => a
            .iter()
            .map(|b| match b {
                Value::U8(b) => Some(*b),
                _ => None,
            })
            .collect(),
        Value::Value(inner) => bytes(inner),
        _ => None,
    }
}

fn bytes_to_path(bytes: &[u8]) -> Option<PathBuf> {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    (end > 0).then(|| PathBuf::from(OsStr::from_bytes(&bytes[..end])))
}

fn string_prop(props: &Props, key: &str) -> Option<String> {
    match props.get(key).map(|v| &**v) {
        Some(Value::Str(s)) => Some(s.to_string()),
        _ => None,
    }
}

/// An object path property; `/` means "none".
fn object_prop(props: &Props, key: &str) -> Option<String> {
    match props.get(key).map(|v| &**v) {
        Some(Value::ObjectPath(p)) if p.as_str() != "/" => Some(p.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions() {
        assert_eq!(parse_version("2.10.1"), Some((2, 10)));
        assert_eq!(parse_version("2.9.0"), Some((2, 9)));
        assert!(parse_version("2.8.4") < Some(MIN_VERSION));
        assert!(parse_version("2.10.1") >= Some(MIN_VERSION));
        assert_eq!(parse_version("garbage"), None);
    }

    #[test]
    fn byte_paths_drop_the_trailing_nul() {
        assert_eq!(
            bytes_to_path(b"/dev/loop0\0"),
            Some(PathBuf::from("/dev/loop0"))
        );
        assert_eq!(bytes_to_path(b"\0"), None);
    }

    #[test]
    fn wrong_tcrypt_passwords_are_recognised() {
        let e = unlock_error(Error::CommandFailed(
            "Error unlocking /dev/loop0: Failed to activate device: Operation not permitted".into(),
        ));
        assert!(matches!(e, Error::WrongPassword));
    }
}
