//! Start Cryptdeck when the user logs in.
//!
//! On Linux this is an XDG autostart entry (`~/.config/autostart`), which
//! GNOME, KDE and most other desktops honour. The entry starts Cryptdeck with
//! `--background`, so it goes straight to the tray instead of opening a
//! window. Inside a Flatpak the sandbox can't write that folder, and other
//! platforms aren't supported yet, so [`supported`] is false there.

use std::io;

/// Command-line flag: start in the tray without opening the window.
pub const BACKGROUND_FLAG: &str = "--background";

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    const ENTRY_NAME: &str = "io.github.m411s.Cryptdeck.desktop";

    pub fn supported() -> bool {
        !Path::new("/.flatpak-info").exists() && entry_path().is_some()
    }

    fn entry_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("autostart").join(ENTRY_NAME))
    }

    /// The program to start. For an AppImage that's the `.AppImage` file, not
    /// the temporary mount the running copy lives in.
    fn program() -> io::Result<PathBuf> {
        match std::env::var_os("APPIMAGE") {
            Some(p) if !p.is_empty() => Ok(PathBuf::from(p)),
            _ => std::env::current_exe(),
        }
    }

    pub fn set(enabled: bool) -> io::Result<()> {
        let path = entry_path()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no config folder"))?;
        if !enabled {
            return match fs::remove_file(&path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            };
        }
        let program = program()?;
        let program = program.to_str().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "program path isn't valid UTF-8")
        })?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("desktop.tmp");
        fs::write(&tmp, entry(program))?;
        fs::rename(&tmp, &path)
    }

    /// The contents of the autostart entry.
    pub fn entry(program: &str) -> String {
        format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=Cryptdeck\n\
             Comment=Unlock and lock VeraCrypt volumes\n\
             Exec={} {BACKGROUND_FLAG}\n\
             Icon=cryptdeck\n\
             Terminal=false\n\
             StartupWMClass=io.github.m411s.Cryptdeck\n\
             X-GNOME-Autostart-enabled=true\n",
            exec_quote(program)
        )
    }

    /// Quote one argument for an `Exec=` line (Desktop Entry spec): wrap in
    /// double quotes and backslash-escape `"`, `` ` ``, `$` and `\`. The
    /// file format then needs every backslash doubled again, and `%` written
    /// as `%%` so it isn't read as a field code.
    pub fn exec_quote(arg: &str) -> String {
        let mut out = String::from("\"");
        for c in arg.chars() {
            match c {
                '"' | '`' | '$' => {
                    out.push_str("\\\\");
                    out.push(c);
                }
                '\\' => out.push_str("\\\\\\\\"),
                '%' => out.push_str("%%"),
                _ => out.push(c),
            }
        }
        out.push('"');
        out
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn quotes_special_characters() {
            assert_eq!(exec_quote("/a b/c"), "\"/a b/c\"");
            assert_eq!(exec_quote("/a\"b"), "\"/a\\\\\"b\"");
            assert_eq!(exec_quote("/a$b`c"), "\"/a\\\\$b\\\\`c\"");
            assert_eq!(exec_quote("/a\\b"), "\"/a\\\\\\\\b\"");
            assert_eq!(exec_quote("/100%"), "\"/100%%\"");
        }

        #[test]
        fn entry_starts_in_background() {
            let e = entry("/home/me/My Apps/Cryptdeck.AppImage");
            assert!(e.contains(
                "Exec=\"/home/me/My Apps/Cryptdeck.AppImage\" --background\n"
            ));
            assert!(e.starts_with("[Desktop Entry]\n"));
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use super::*;

    pub fn supported() -> bool {
        false
    }

    pub fn set(_enabled: bool) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "starting at login isn't supported on this platform yet",
        ))
    }
}

/// Whether this platform and packaging can start Cryptdeck at login.
pub fn supported() -> bool {
    imp::supported()
}

/// Turn starting at login on or off. Turning it on also repairs an entry
/// that points at an old location (for example after moving the AppImage).
pub fn set(enabled: bool) -> io::Result<()> {
    imp::set(enabled)
}
