//! Backend that drives the `veracrypt` command-line tool in text mode.
//!
//! Works on Linux and macOS, where VeraCrypt ships a scriptable CLI. The
//! password is written to the child's stdin (`--stdin`), so it never appears
//! in the process list.

use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use secrecy::ExposeSecret;
use zeroize::Zeroizing;

use crate::{Backend, Error, MountRequest, MountedVolume, Result};

/// How to obtain root for operations that need it (mount / dismount).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Elevation {
    /// Run `veracrypt` directly. Use when already root, or on platforms where
    /// the CLI elevates by itself.
    None,
    /// Wrap the command in `pkexec`, which shows the desktop's polkit
    /// authentication dialog.
    Pkexec,
}

impl Elevation {
    /// Sensible default for the current process.
    pub fn detect() -> Self {
        if cfg!(target_os = "linux") && !is_root() {
            Elevation::Pkexec
        } else {
            Elevation::None
        }
    }
}

#[derive(Debug, Clone)]
pub struct VeraCryptCli {
    binary: OsString,
    elevation: Elevation,
}

impl Default for VeraCryptCli {
    fn default() -> Self {
        Self::new()
    }
}

impl VeraCryptCli {
    pub fn new() -> Self {
        Self {
            binary: std::env::var_os("CRYPTDECK_VERACRYPT").unwrap_or_else(|| "veracrypt".into()),
            elevation: Elevation::detect(),
        }
    }

    pub fn with_elevation(mut self, elevation: Elevation) -> Self {
        self.elevation = elevation;
        self
    }

    fn resolve_binary(&self) -> Result<PathBuf> {
        find_executable(&self.binary)
            .ok_or_else(|| Error::NotInstalled(self.binary.to_string_lossy().into_owned()))
    }

    /// Build a command, optionally elevated.
    fn command(&self, elevated: bool) -> Result<Command> {
        let binary = self.resolve_binary()?;
        if elevated && self.elevation == Elevation::Pkexec {
            let pkexec = find_executable(OsStr::new("pkexec"))
                .ok_or_else(|| Error::NotInstalled("pkexec (polkit)".into()))?;
            let mut cmd = Command::new(pkexec);
            // pkexec requires an absolute program path.
            cmd.arg(binary);
            Ok(cmd)
        } else {
            Ok(Command::new(binary))
        }
    }

    fn run(&self, elevated: bool, args: &[OsString], stdin: Option<&[u8]>) -> Result<Output> {
        let mut cmd = self.command(elevated)?;
        cmd.args(args)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn()?;
        if let Some(data) = stdin {
            let mut pipe = child.stdin.take().expect("stdin was piped");
            // A write error here usually means the child exited early; its
            // stderr will explain why, so let wait_with_output report it.
            let _ = pipe.write_all(data);
            drop(pipe);
        }
        let output = child.wait_with_output()?;

        if output.status.success() {
            return Ok(output);
        }
        if elevated && self.elevation == Elevation::Pkexec {
            // pkexec: 126 = dialog dismissed, 127 = not authorized.
            if let Some(126 | 127) = output.status.code() {
                return Err(Error::AuthCancelled);
            }
        }
        Err(Error::CommandFailed(error_message(&output)))
    }
}

impl Backend for VeraCryptCli {
    fn name(&self) -> &str {
        "VeraCrypt CLI"
    }

    fn check(&self) -> Result<String> {
        let out = self.run(false, &args(&["--text", "--version"]), None)?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    }

    fn list(&self) -> Result<Vec<MountedVolume>> {
        match self.run(
            false,
            &args(&["--text", "--non-interactive", "--list"]),
            None,
        ) {
            Ok(out) => Ok(parse_list(&String::from_utf8_lossy(&out.stdout))),
            Err(Error::CommandFailed(msg)) if msg.contains("No volumes mounted") => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    fn mount(&self, request: &MountRequest) -> Result<()> {
        request.validate()?;
        let args = mount_args(request)?;

        let password = request.password.expose_secret().as_bytes();
        let mut stdin = Zeroizing::new(Vec::with_capacity(password.len() + 1));
        stdin.extend_from_slice(password);
        stdin.push(b'\n');

        self.run(true, &args, Some(&stdin))?;
        Ok(())
    }

    fn dismount(&self, volume: &MountedVolume) -> Result<()> {
        let mut a = args(&["--text", "--non-interactive", "--dismount"]);
        a.push(volume.volume.clone().into_os_string());
        self.run(true, &a, None)?;
        Ok(())
    }

    fn dismount_all(&self) -> Result<()> {
        self.run(
            true,
            &args(&["--text", "--non-interactive", "--dismount"]),
            None,
        )?;
        Ok(())
    }
}

fn args(list: &[&str]) -> Vec<OsString> {
    list.iter().map(OsString::from).collect()
}

/// Build the argument list for a mount. Kept separate so it can be unit tested.
pub(crate) fn mount_args(req: &MountRequest) -> Result<Vec<OsString>> {
    let mut a = args(&[
        "--text",
        "--non-interactive",
        "--stdin",
        "--protect-hidden=no",
    ]);

    // Always pass PIM and keyfiles explicitly so VeraCrypt never prompts.
    a.push(format!("--pim={}", req.pim.unwrap_or(0)).into());

    let mut keyfiles = OsString::new();
    for (i, kf) in req.keyfiles.iter().enumerate() {
        if i > 0 {
            keyfiles.push(",");
        }
        keyfiles.push(absolute(kf)?);
    }
    a.push("--keyfiles".into());
    a.push(keyfiles);

    if req.read_only {
        a.push("--mount-options=ro".into());
    }

    // Absolute paths always start with '/', so they can't be mistaken for options.
    a.push(absolute(&req.volume)?.into_os_string());
    if let Some(mp) = &req.mount_point {
        a.push(absolute(mp)?.into_os_string());
    }
    Ok(a)
}

fn absolute(path: &Path) -> Result<PathBuf> {
    Ok(std::path::absolute(path)?)
}

/// Parse `veracrypt --text --list` output. Each line looks like:
///
/// ```text
/// 1: /home/me/vault.hc /dev/mapper/veracrypt1 /media/veracrypt1
/// ```
///
/// Paths may contain spaces, so the virtual device (the first token starting
/// with `/dev/` after the volume path) is used as the separator.
pub(crate) fn parse_list(output: &str) -> Vec<MountedVolume> {
    output.lines().filter_map(parse_list_line).collect()
}

fn parse_list_line(line: &str) -> Option<MountedVolume> {
    let (slot, rest) = line.trim().split_once(": ")?;
    let slot = slot.trim().parse().ok()?;

    let dev_start = rest.find(" /dev/")?;
    let volume = &rest[..dev_start];
    let after_volume = &rest[dev_start + 1..];
    let (device, mount_point) = match after_volume.split_once(' ') {
        Some((d, m)) => (d, m.trim()),
        None => (after_volume, ""),
    };

    let mount_point = match mount_point {
        "" | "-" => None,
        m => Some(PathBuf::from(m)),
    };
    Some(MountedVolume {
        slot,
        volume: PathBuf::from(volume),
        virtual_device: PathBuf::from(device),
        mount_point,
    })
}

/// Turn a failed command's output into a short message for the user.
fn error_message(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let text = if stderr.trim().is_empty() {
        stdout
    } else {
        stderr
    };
    let text = text.trim();
    let text = text.strip_prefix("Error: ").unwrap_or(text);
    if text.is_empty() {
        format!("veracrypt exited with {}", output.status)
    } else {
        text.to_owned()
    }
}

/// Locate an executable on PATH (or accept an explicit path).
fn find_executable(name: &OsStr) -> Option<PathBuf> {
    let candidate = Path::new(name);
    if candidate.components().count() > 1 {
        return candidate.is_file().then(|| candidate.to_path_buf());
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|p| is_executable(p))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(unix)]
fn is_root() -> bool {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc::geteuid() == 0 }
}

#[cfg(not(unix))]
fn is_root() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use secrecy::SecretString;

    #[test]
    fn parses_list_output() {
        let out = "1: /home/me/vault.hc /dev/mapper/veracrypt1 /media/veracrypt1\n\
                   2: /dev/sdb1 /dev/mapper/veracrypt2 /mnt/usb\n";
        let vols = parse_list(out);
        assert_eq!(vols.len(), 2);
        assert_eq!(vols[0].slot, 1);
        assert_eq!(vols[0].volume, PathBuf::from("/home/me/vault.hc"));
        assert_eq!(
            vols[0].virtual_device,
            PathBuf::from("/dev/mapper/veracrypt1")
        );
        assert_eq!(
            vols[0].mount_point,
            Some(PathBuf::from("/media/veracrypt1"))
        );
        assert_eq!(vols[1].volume, PathBuf::from("/dev/sdb1"));
    }

    #[test]
    fn parses_paths_with_spaces() {
        let vols =
            parse_list("3: /home/me/My Files/v 1.hc /dev/mapper/veracrypt3 /media/my vault\n");
        assert_eq!(vols[0].volume, PathBuf::from("/home/me/My Files/v 1.hc"));
        assert_eq!(vols[0].mount_point, Some(PathBuf::from("/media/my vault")));
    }

    #[test]
    fn parses_volume_without_mount_point() {
        let vols = parse_list(
            "1: /home/me/v.hc /dev/mapper/veracrypt1 -\n4: /x.hc /dev/mapper/veracrypt4\n",
        );
        assert_eq!(vols[0].mount_point, None);
        assert_eq!(vols[1].mount_point, None);
    }

    #[test]
    fn ignores_junk_lines() {
        assert!(parse_list("No volumes mounted.\n\n").is_empty());
    }

    // Uses Unix paths; the veracrypt CLI backend targets Linux and macOS.
    #[test]
    #[cfg(unix)]
    fn mount_args_never_contain_password() {
        let req = MountRequest {
            volume: "/vols/a.hc".into(),
            mount_point: Some("/mnt/a".into()),
            password: SecretString::from("hunter2"),
            pim: Some(500),
            keyfiles: vec!["/k/one".into(), "/k/two".into()],
            read_only: true,
        };
        let a: Vec<String> = mount_args(&req)
            .unwrap()
            .into_iter()
            .map(|s| s.into_string().unwrap())
            .collect();
        assert!(!a.iter().any(|s| s.contains("hunter2")));
        assert!(a.contains(&"--stdin".to_string()));
        assert!(a.contains(&"--pim=500".to_string()));
        assert!(a.contains(&"/k/one,/k/two".to_string()));
        assert!(a.contains(&"--mount-options=ro".to_string()));
        assert_eq!(&a[a.len() - 2..], ["/vols/a.hc", "/mnt/a"]);
    }

    #[test]
    #[cfg(unix)]
    fn mount_args_defaults_avoid_prompts() {
        let req = MountRequest {
            volume: "/vols/a.hc".into(),
            mount_point: None,
            password: SecretString::from("pw"),
            pim: None,
            keyfiles: vec![],
            read_only: false,
        };
        let a = mount_args(&req).unwrap();
        assert!(a.contains(&OsString::from("--pim=0")));
        let k = a.iter().position(|s| s == "--keyfiles").unwrap();
        assert_eq!(a[k + 1], "");
        assert_eq!(a.last().unwrap(), "/vols/a.hc");
    }
}
