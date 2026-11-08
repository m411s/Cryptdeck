//! Backend for Windows: VeraCrypt.exe unlocks and locks volumes, and the
//! VeraCrypt driver lists what's unlocked.
//!
//! VeraCrypt on Windows works through its own signed kernel driver, so it
//! must be installed. VeraCrypt.exe has no command to list volumes, so the
//! list comes straight from the driver (`\\.\VeraCrypt`), which reports
//! each drive letter with the container or device behind it.
//!
//! Limitation: VeraCrypt.exe only takes the password as a command-line
//! argument (`/password`), and programs running as the same user, and
//! administrators, can read a running program's command line. Monitoring
//! tools that log process command lines would record it. Passing it to the
//! driver directly would avoid this; see the README.

// The helpers below are plain Rust so they're unit-tested on every
// platform, but only the Windows backend uses them.
#![cfg_attr(not(windows), allow(dead_code))]

use std::path::PathBuf;

use crate::{Error, MountRequest, Result};

/// Size of the driver's path buffers, in UTF-16 units (Windows `MAX_PATH`).
const DRIVER_PATH_LEN: usize = 260;

/// Parse the start of the driver's mount list: a little-endian `u32` bit
/// mask of mounted drive letters (bit 0 is A:), followed by one
/// `DRIVER_PATH_LEN`-long UTF-16 path per letter A..Z. Only those two
/// fields are read; they come first in every VeraCrypt version, while the
/// fields after them have changed over time.
///
/// Container paths come back as `\??\C:\…`; the prefix is dropped.
pub(crate) fn parse_mount_list(buf: &[u8]) -> Vec<(char, String)> {
    let Some(mask) = buf
        .get(..4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    else {
        return Vec::new();
    };
    (0..26)
        .filter(|i| mask & (1 << i) != 0)
        .filter_map(|i| {
            let start = 4 + i * DRIVER_PATH_LEN * 2;
            let raw = buf.get(start..start + DRIVER_PATH_LEN * 2)?;
            let units: Vec<u16> = raw
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .take_while(|&u| u != 0)
                .collect();
            let path = String::from_utf16_lossy(&units);
            let path = path.strip_prefix(r"\??\").unwrap_or(&path).to_string();
            Some((char::from(b'A' + i as u8), path))
        })
        .collect()
}

/// The first free drive letter, searching from Z: down to D:, like
/// VeraCrypt's own suggestions. `used` is a `GetLogicalDrives` bit mask.
pub(crate) fn free_drive_letter(used: u32) -> Option<char> {
    (3..26)
        .rev()
        .find(|i| used & (1 << i) == 0)
        .map(|i| char::from(b'A' + i as u8))
}

/// Arguments for VeraCrypt.exe to mount `req` as `letter`.
pub(crate) fn mount_args(req: &MountRequest, letter: char) -> Vec<String> {
    use secrecy::ExposeSecret;

    let mut args = vec![
        "/volume".into(),
        req.volume.display().to_string(),
        "/letter".into(),
        letter.to_string(),
        "/password".into(),
        req.password.expose_secret().to_string(),
    ];
    if req.password.expose_secret().is_empty() {
        // Keyfiles only: let VeraCrypt try an empty password.
        args.extend(["/tryemptypass".into(), "y".into()]);
    }
    if let Some(pim) = req.pim {
        args.extend(["/pim".into(), pim.to_string()]);
    }
    for kf in &req.keyfiles {
        args.extend(["/keyfile".into(), kf.display().to_string()]);
    }
    if req.read_only {
        args.extend(["/mountoption".into(), "ro".into()]);
    }
    // Don't cache the password in the driver, don't add the volume to
    // VeraCrypt's history, show no windows, and exit when done. A hidden
    // volume needs no flag: VeraCrypt tries its header by itself.
    args.extend(
        ["/cache", "n", "/history", "n", "/quit", "/silent"]
            .iter()
            .map(|s| s.to_string()),
    );
    args
}

#[cfg(windows)]
pub use imp::VeraCryptWin;

#[cfg(windows)]
mod imp {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, GetLogicalDrives, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;

    use super::{free_drive_letter, mount_args, parse_mount_list, veracrypt_exe};
    use crate::{Backend, Error, MountRequest, MountedVolume, Result};

    /// `CTL_CODE(FILE_DEVICE_UNKNOWN, 0x800 + 6, METHOD_BUFFERED,
    /// FILE_ANY_ACCESS)`: VeraCrypt's "get mounted volumes" request.
    const IOCTL_GET_MOUNTED_VOLUMES: u32 = (0x22 << 16) | ((0x800 + 6) << 2);
    /// Bigger than the driver's reply in any version (about 17 KB today).
    const LIST_BUFFER_SIZE: usize = 64 * 1024;
    /// Run VeraCrypt.exe without flashing a console window.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub struct VeraCryptWin;

    impl VeraCryptWin {
        pub fn new() -> Self {
            Self
        }

        fn run(&self, args: &[String]) -> Result<bool> {
            let exe = veracrypt_exe()?;
            let status = Command::new(&exe)
                .args(args)
                .creation_flags(CREATE_NO_WINDOW)
                .status()?;
            Ok(status.success())
        }

        /// Ask the driver which drive letters hold VeraCrypt volumes.
        fn mounted(&self) -> Result<Vec<(char, String)>> {
            let name: Vec<u16> = Path::new(r"\\.\VeraCrypt")
                .as_os_str()
                .encode_wide()
                .chain([0])
                .collect();
            // SAFETY: `name` is a NUL-terminated UTF-16 string that outlives
            // the call; the other arguments are plain values or null.
            let handle = unsafe {
                CreateFileW(
                    name.as_ptr(),
                    0,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    std::ptr::null(),
                    OPEN_EXISTING,
                    0,
                    std::ptr::null_mut(),
                )
            };
            if handle == INVALID_HANDLE_VALUE {
                // No driver running: nothing can be mounted.
                return Ok(Vec::new());
            }
            // The driver reads its request from the same structure it
            // fills in, so the buffer is both input and output.
            let mut buf = vec![0u8; LIST_BUFFER_SIZE];
            let mut returned = 0u32;
            // SAFETY: `handle` is open; `buf` is valid for reads and writes
            // of its full length for the duration of this synchronous call,
            // and the input and output pointers may alias for
            // METHOD_BUFFERED requests.
            let ok = unsafe {
                DeviceIoControl(
                    handle,
                    IOCTL_GET_MOUNTED_VOLUMES,
                    buf.as_ptr().cast(),
                    buf.len() as u32,
                    buf.as_mut_ptr().cast(),
                    buf.len() as u32,
                    &mut returned,
                    std::ptr::null_mut(),
                )
            };
            let error = std::io::Error::last_os_error();
            // SAFETY: `handle` was opened above and is closed exactly once.
            unsafe { CloseHandle(handle) };
            if ok == 0 {
                return Err(Error::CommandFailed(format!(
                    "Couldn't ask VeraCrypt which volumes are unlocked ({error})"
                )));
            }
            Ok(parse_mount_list(&buf[..returned as usize]))
        }
    }

    impl Default for VeraCryptWin {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Backend for VeraCryptWin {
        fn name(&self) -> &str {
            "VeraCrypt"
        }

        fn check(&self) -> Result<String> {
            let exe = veracrypt_exe()?;
            Ok(format!(
                "Using VeraCrypt ({})",
                exe.parent().unwrap_or(&exe).display()
            ))
        }

        fn supports_mount_point(&self) -> bool {
            false
        }

        fn list(&self) -> Result<Vec<MountedVolume>> {
            Ok(self
                .mounted()?
                .into_iter()
                .map(|(letter, path)| MountedVolume {
                    slot: u32::from(letter as u8 - b'A'),
                    volume: PathBuf::from(path),
                    virtual_device: PathBuf::from(format!("{letter}:")),
                    mount_point: Some(PathBuf::from(format!("{letter}:\\"))),
                })
                .collect())
        }

        fn mount(&self, request: &MountRequest) -> Result<()> {
            request.validate()?;
            let volume = request
                .volume
                .canonicalize()
                .unwrap_or(request.volume.clone());
            if self.list()?.iter().any(|v| same_path(&v.volume, &volume)) {
                return Err(Error::AlreadyMounted);
            }
            // SAFETY: GetLogicalDrives takes no arguments and can't fail
            // in a way that matters here (0 just means "none in use").
            let used = unsafe { GetLogicalDrives() };
            let letter = free_drive_letter(used)
                .ok_or_else(|| Error::CommandFailed("No free drive letter".into()))?;
            let mut req_args = mount_args(request, letter);
            // VeraCrypt resolves paths itself; give it the plain form.
            req_args[1] = plain(&volume);
            if self.run(&req_args)? {
                Ok(())
            } else {
                Err(Error::CommandFailed(
                    "VeraCrypt couldn't unlock the volume. Check the password, PIM and \
                     keyfiles (and \"Open the hidden volume\" for a hidden one). If they're \
                     right, try unlocking it in VeraCrypt itself to see its full message."
                        .into(),
                ))
            }
        }

        fn dismount(&self, volume: &MountedVolume) -> Result<()> {
            let letter = volume
                .virtual_device
                .to_string_lossy()
                .chars()
                .next()
                .ok_or_else(|| Error::CommandFailed("Unknown drive letter".into()))?;
            let args: Vec<String> = ["/dismount", &letter.to_string(), "/quit", "/silent"]
                .iter()
                .map(|s| s.to_string())
                .collect();
            if self.run(&args)? {
                Ok(())
            } else {
                Err(Error::VolumeBusy)
            }
        }

        fn dismount_all(&self) -> Result<()> {
            let args: Vec<String> = ["/dismount", "/quit", "/silent"]
                .iter()
                .map(|s| s.to_string())
                .collect();
            if self.run(&args)? {
                Ok(())
            } else {
                Err(Error::VolumeBusy)
            }
        }
    }

    /// `canonicalize` on Windows adds a `\\?\` prefix; VeraCrypt and the
    /// driver's list use plain paths.
    fn plain(path: &Path) -> String {
        let s = path.display().to_string();
        s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
    }

    fn same_path(a: &Path, b: &Path) -> bool {
        plain(a).eq_ignore_ascii_case(&plain(b))
    }
}

/// Find VeraCrypt.exe: `CRYPTDECK_VERACRYPT`, or the usual install folders.
fn veracrypt_exe() -> Result<PathBuf> {
    if let Some(p) = std::env::var_os("CRYPTDECK_VERACRYPT") {
        return Ok(PathBuf::from(p));
    }
    ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|dir| PathBuf::from(dir).join("VeraCrypt").join("VeraCrypt.exe"))
        .find(|p| p.is_file())
        .ok_or_else(|| Error::NotInstalled("VeraCrypt".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::SecretString;

    fn list_buffer(entries: &[(usize, &str)]) -> Vec<u8> {
        let mut buf = vec![0u8; 4 + 26 * DRIVER_PATH_LEN * 2 + 512];
        let mut mask = 0u32;
        for &(i, path) in entries {
            mask |= 1 << i;
            let start = 4 + i * DRIVER_PATH_LEN * 2;
            for (j, unit) in path.encode_utf16().enumerate() {
                buf[start + 2 * j..start + 2 * j + 2].copy_from_slice(&unit.to_le_bytes());
            }
        }
        buf[..4].copy_from_slice(&mask.to_le_bytes());
        buf
    }

    #[test]
    fn parses_the_driver_mount_list() {
        let buf = list_buffer(&[
            (25, r"\??\C:\Users\Matt\Documents\Private Vault.hc"),
            (12, r"\Device\Harddisk1\Partition2"),
        ]);
        assert_eq!(
            parse_mount_list(&buf),
            vec![
                ('M', r"\Device\Harddisk1\Partition2".to_string()),
                ('Z', r"C:\Users\Matt\Documents\Private Vault.hc".to_string()),
            ]
        );
    }

    #[test]
    fn short_or_empty_buffers_are_harmless() {
        assert!(parse_mount_list(&[]).is_empty());
        assert!(parse_mount_list(&[0, 0, 0, 0]).is_empty());
        // Letter claimed but its path is cut off: skipped, not a panic.
        assert!(parse_mount_list(&[1, 0, 0, 0, 65, 0]).is_empty());
    }

    #[test]
    fn picks_free_letters_from_z_down() {
        // A:, C:, D: and Z: in use.
        let used = 1 | (1 << 2) | (1 << 3) | (1 << 25);
        assert_eq!(free_drive_letter(used), Some('Y'));
        assert_eq!(free_drive_letter(u32::MAX), None);
    }

    #[test]
    fn mount_args_are_complete() {
        let req = MountRequest {
            volume: r"C:\v.hc".into(),
            mount_point: None,
            password: SecretString::from("pw"),
            pim: Some(7),
            keyfiles: vec![r"C:\k1".into()],
            read_only: true,
            hidden: true,
        };
        let a = mount_args(&req, 'X').join(" ");
        assert_eq!(
            a,
            r"/volume C:\v.hc /letter X /password pw /pim 7 /keyfile C:\k1 /mountoption ro /cache n /history n /quit /silent"
        );
    }

    #[test]
    fn keyfile_only_tries_an_empty_password() {
        let req = MountRequest {
            volume: r"C:\v.hc".into(),
            mount_point: None,
            password: SecretString::from(""),
            pim: None,
            keyfiles: vec![r"C:\k1".into()],
            read_only: false,
            hidden: false,
        };
        assert!(mount_args(&req, 'X').join(" ").contains("/tryemptypass y"));
    }
}
