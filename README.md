# Cryptdeck

A cross-platform desktop GUI for VeraCrypt volumes, written in Rust with
[iced](https://iced.rs).

![Cryptdeck with one volume unlocked](docs/screenshot.png)

> **Status: early prototype.** Linux works through the VeraCrypt CLI.
> macOS and Windows builds run, but can't unlock volumes yet. Don't trust it
> with data you can't afford to lose yet.

## Using Cryptdeck

1. Install [VeraCrypt](https://www.veracrypt.fr/en/Downloads.html).
2. Download Cryptdeck from the
   [Releases page](https://github.com/m411s/Cryptdeck/releases). On Linux,
   take the AppImage, make it executable (`chmod +x Cryptdeck-*.AppImage`)
   and run it.
3. Choose **Add volume…**, or drag a volume file onto the window.
4. Enter the password and choose **Unlock**. If you set a PIM or keyfiles
   when creating the volume, add them under **Advanced options**.
5. Choose **Open** to see the files, and **Lock** when you're done.

Cryptdeck remembers the volumes you add (just their location, never
passwords), so next time you only need to choose **Unlock**. Unlocked
volumes stay unlocked until you lock them, even after you close Cryptdeck.

Cryptdeck is an independent project. It is not affiliated with or endorsed
by IDRIX or the VeraCrypt project.

## How it works

Cryptdeck doesn't implement any cryptography. It drives an existing, audited
tool as a separate process:

| Platform | Backend | Status |
| --- | --- | --- |
| Linux | `veracrypt --text` CLI, password passed on stdin | Working |
| Linux | `cryptsetup --type tcrypt --veracrypt` (no VeraCrypt install needed) | Planned |
| macOS | `veracrypt --text` CLI | Planned (should need little work) |
| Windows | VeraCrypt driver via `DeviceIoControl` | Planned |

Each backend implements the `Backend` trait in `cryptdeck-core`, and the GUI
only talks to that trait.

### Security notes

- The password is written to VeraCrypt's stdin (`--stdin`), so it never
  appears in the process list or in shell history.
- Inside Cryptdeck, the password is held in `secrecy`/`zeroize` types and
  wiped after a successful mount. iced's text input keeps its own copy while
  you're typing, and Cryptdeck can't wipe that copy.
- Mounting needs root. When Cryptdeck isn't running as root, it runs
  `veracrypt` through `pkexec`, which shows your desktop's polkit password
  dialog. A dedicated, narrowly scoped helper with its own polkit policy is
  on the roadmap.
- Before running anything as root, Cryptdeck checks that the `veracrypt`
  binary and its folder are owned by root and not writable by other users,
  so a program running as you can't plant a fake `veracrypt` (for example in
  `~/.local/bin`) and gain root the next time you authenticate. `pkexec` is
  only ever taken from `/usr/bin` or `/bin`, never from your `PATH`.

## Building

You need Rust 1.85 or newer.

```bash
cargo build --release
```

On Linux you need VeraCrypt (the console or GUI build) and polkit (`pkexec`)
installed.

### Trying it without VeraCrypt

The mock backend keeps everything in memory. It accepts any existing file as
a volume, and the password is `test`:

```bash
CRYPTDECK_BACKEND=mock cargo run
```

### Environment variables

| Variable | Effect |
| --- | --- |
| `CRYPTDECK_BACKEND=mock` | Use the in-memory demo backend |
| `CRYPTDECK_VERACRYPT=/path/to/veracrypt` | Use a specific `veracrypt` binary (it must be root-owned to mount) |
| `CRYPTDECK_THEME=light` or `dark` | Override the system light/dark mode |
| `CRYPTDECK_CONFIG_DIR=/path` | Keep the saved volume list somewhere else |

The saved volume list is `volumes.json` in your config folder:
`~/.config/cryptdeck` on Linux, `~/Library/Application Support/cryptdeck`
on macOS, and `%APPDATA%\cryptdeck` on Windows.

## Layout

```
crates/
  cryptdeck-core/   Backend trait, VeraCrypt CLI backend, mock backend
  cryptdeck-gui/    iced desktop app (binary: cryptdeck)
assets/             App icon (SVG source and rendered PNG, ICO, RGBA)
packaging/          AppImage, macOS .app/.dmg, and release notes
```

## Releasing

**One click (recommended):** open **Actions → Bump version and release →
Run workflow**, pick `patch` (0.1.0 → 0.1.1), `minor` (0.1.0 → 0.2.0) or
`major` (0.1.0 → 1.0.0), and choose **Run workflow**. It updates the version
in `Cargo.toml` and `Cargo.lock`, commits that to `main`, then builds and
publishes the release.

**Automatic:** when a push to `main` changes the version in `Cargo.toml` to
one that hasn't been released, the release is built and published by
itself. Run `cargo build` after editing the version, so `Cargo.lock` is
updated too, and commit both.

**By hand:** run the **Release** workflow from the Actions tab and enter the
current version, or push a matching tag:

```bash
git tag -a v0.2.0 -m "Cryptdeck 0.2.0"
```

```bash
git push origin v0.2.0
```

Each release has these files and a `SHA256SUMS.txt`:

| File | Notes |
| --- | --- |
| `Cryptdeck-<version>-x86_64.AppImage` | Linux, glibc 2.35 or newer (Ubuntu 22.04, Debian 12, Fedora 36 and later) |
| `cryptdeck-<version>-linux-x86_64.tar.gz` | Linux, plain binary |
| `Cryptdeck-<version>-macos-universal.dmg` | macOS 11+, Apple Silicon and Intel, ad-hoc signed |
| `Cryptdeck-<version>-windows-x86_64.zip` | Windows 10+, unsigned |

The version must match `Cargo.toml` and must not have been released before,
or the workflow stops before building. A version with a hyphen
(`0.2.0-beta.1`) is published as a pre-release. Running the workflow without
a version, or pushing changes to `packaging/`, `assets/` or the workflow
itself, runs the same builds without publishing and leaves the files as
workflow artifacts.

## Roadmap

- [x] List, mount and dismount (Linux, VeraCrypt CLI)
- [x] PIM, keyfiles, read-only mounts
- [x] Remembered volumes, drag and drop, plain-language errors
- [x] Release builds: AppImage, macOS .dmg, Windows .zip
- [ ] polkit helper with a restricted policy instead of a raw `pkexec veracrypt`
- [ ] Mount ownership for non-root users (FAT, exFAT and NTFS `uid`/`gid` options)
- [ ] Hidden volume protection
- [ ] System tray
- [ ] Developer ID signing and notarization (macOS), code signing (Windows)
- [ ] `cryptsetup` backend
- [ ] Volume creation wizard
- [ ] macOS
- [ ] Windows

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Cryptdeck runs VeraCrypt as a separate program and contains no VeraCrypt
code, so VeraCrypt's licenses (Apache 2.0 and the TrueCrypt License 3.0) do
not apply to it.
