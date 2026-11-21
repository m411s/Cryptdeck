# Cryptdeck

A cross-platform desktop GUI for VeraCrypt volumes, written in Rust with
[iced](https://iced.rs).

![Cryptdeck with one volume unlocked](docs/screenshot.png)

> [!WARNING]
> **Early prototype, use at your own risk.** Linux works (through udisks2,
> or the VeraCrypt CLI). Windows support is new and still being tested, and
> macOS builds run but can't unlock volumes yet.
>
> Cryptdeck is provided "as is", without warranty of any kind, and its
> authors accept **no responsibility for any data loss** or other damage
> (see [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE)).
> Keep backups of anything you can't afford to lose: copies of your files,
> and VeraCrypt's backup of each volume's header (VeraCrypt: **Tools →
> Backup Volume Header**). Nobody can recover a volume whose password
> you've forgotten. Cryptdeck is an independent project, not made, endorsed
> or supported by IDRIX or the VeraCrypt project.

## Using Cryptdeck

1. On Linux, Cryptdeck uses your system's disk service (udisks2), so you
   don't need VeraCrypt. Many distributions (Ubuntu, Debian, Fedora) ship
   udisks2 with VeraCrypt detection switched off; if so, Cryptdeck shows a
   **Turn on VeraCrypt support** button that fixes it once, with one
   administrator password (see [udisks2 setup](#udisks2-setup-linux)).
   Elsewhere, or if udisks2 isn't available, install
   [VeraCrypt](https://www.veracrypt.fr/en/Downloads.html).
2. Download Cryptdeck from the
   [Releases page](https://github.com/m411s/Cryptdeck/releases). On Linux,
   take the AppImage, make it executable (`chmod +x Cryptdeck-*.AppImage`)
   and run it.
   Or install the Flatpak from the same page (see [Flatpak](#flatpak) below).
3. Choose **Add volume…**, or drag a volume file onto the window.
4. Enter the password and choose **Unlock**. If you set a PIM or keyfiles
   when creating the volume, add them under **Advanced options**.
5. Choose **Open** to see the files, and **Lock** when you're done.

Cryptdeck remembers the volumes you add (just their location, never
passwords), so next time you only need to choose **Unlock**. Unlocked
volumes stay unlocked until you lock them, even after you close Cryptdeck.

### Flatpak

Each release includes `Cryptdeck-<version>-x86_64.flatpak`. Install it with:

```bash
flatpak install --user ./Cryptdeck-*-x86_64.flatpak
```

It uses your system's udisks2 (there's no VeraCrypt fallback inside the
Flatpak sandbox) and can read volume files in your home folder and on
removable drives under `/media` and `/run/media`. Unlocking whole
partitions or drives (`/dev/…`) isn't available in the Flatpak.

To build it yourself: `packaging/flatpak/build.sh` (add `--bundle` for a
`.flatpak` file).

**Publishing on Flathub** needs the repository to be public. Then fork
[flathub/flathub](https://github.com/flathub/flathub), add the manifest
from `packaging/flatpak/` with a `git` source pointing at a release tag
instead of `type: dir`, plus the generated `cargo-sources.json`, and open
a pull request. Flathub's reviewers check the permissions in the manifest;
each one has a comment explaining why it's needed.

### udisks2 setup (Linux)

A VeraCrypt volume has no visible header: without the password it looks
like random data. udisks2 only checks unrecognised devices for VeraCrypt
when the file `/etc/udisks2/tcrypt.conf` exists (it can be empty), and
reads it when the service starts. **Turn on VeraCrypt support** creates
that file and restarts udisks2 through your desktop's password dialog
(polkit). To do it by hand:

```bash
sudo touch /etc/udisks2/tcrypt.conf
sudo systemctl restart udisks2
```

Restarting udisks2 doesn't unmount anything. The Flatpak can't make this
change from inside its sandbox, so there you'll need the commands above.

### Languages

Cryptdeck is available in English and German, and follows your system
language (set `CRYPTDECK_LANG=de` or `en` to choose). To add a language,
add its code next to `en` and `de` under every key in
`crates/cryptdeck-gui/locales/app.yml` and
`crates/cryptdeck-core/locales/core.yml`. A test fails if any text used by
the app is missing in a language.

### Automatic locking (Linux)

Under **Settings**, Cryptdeck can lock all volumes when the computer goes to
sleep (on by default) or when the screen locks (off by default). It asks the
system to wait a moment before sleeping, locks the volumes, then lets it
sleep. This works while the window is closed, as long as Cryptdeck is
running in the tray. Volumes with files still open can't be locked; you'll
see a message if that happens. It needs systemd-logind, which almost every
Linux distribution uses.

### System tray (Linux)

On desktops with a system tray, Cryptdeck adds an icon there. Its menu
unlocks, opens and locks your volumes, and closing the window keeps
Cryptdeck running in the tray; choose **Quit Cryptdeck** from the menu to
exit. KDE, Xfce, Cinnamon, MATE and Ubuntu's GNOME show it out of the box.
On other GNOME setups, install the "AppIndicator and KStatusNotifierItem
Support" extension. Without a tray, closing the window quits as usual.

### Privacy

Cryptdeck collects nothing and has no servers: no telemetry, analytics,
crash reports or accounts, and it makes no network connections of its own.
It remembers only where your volumes are and your settings, on your
computer. See [PRIVACY.md](PRIVACY.md).

## How it works

Cryptdeck doesn't implement any cryptography. It drives an existing, audited
tool as a separate process:

| Platform | Backend | Status |
| --- | --- | --- |
| Linux | udisks2 (the service behind GNOME Disks and KDE's file manager), which uses `cryptsetup`; no VeraCrypt install needed | Default when udisks 2.9+ is running; needs [one-time setup](#udisks2-setup-linux) on some distributions |
| Linux | `veracrypt --text` CLI, password passed on stdin | Fallback |
| macOS | `veracrypt --text` CLI | Planned (should need little work) |
| Windows | `VeraCrypt.exe` to unlock and lock; the VeraCrypt driver to list unlocked volumes | New: needs VeraCrypt installed |

Each backend implements the `Backend` trait in `cryptdeck-core`, and the GUI
only talks to that trait.

With udisks2, volumes are mounted as you, under `/run/media/<you>`, like a
USB stick, so you can write to them. Your desktop's polkit rules decide
whether an admin password is needed; for your own container files it
usually isn't. udisks always chooses the mount folder, so "Unlock to
folder" isn't offered with it.

### Security notes

- **Windows:** VeraCrypt.exe only accepts the password on its command line,
  so while it runs (a second or two), other programs running as you, and
  administrators, could read it, and monitoring tools that log process
  command lines may record it. On a shared or managed (work) PC, keep this
  in mind. Passing the password to the VeraCrypt driver directly would avoid
  it and is on the roadmap.

- With udisks2, the password goes to the udisks service over the local
  system D-Bus, and Cryptdeck never runs anything as root to unlock or
  lock. The only exception is the one-time **Turn on VeraCrypt support**,
  which runs `pkexec /bin/sh -c` with a fixed command that creates
  `/etc/udisks2/tcrypt.conf` and restarts udisks2. Keyfiles are
  read by Cryptdeck and passed by content (keyfile folders aren't
  supported there).
- With the VeraCrypt CLI, the password is written to VeraCrypt's stdin
  (`--stdin`), so it never appears in the process list or in shell history.
- Inside Cryptdeck, the password is held in `secrecy`/`zeroize` types and
  wiped after a successful mount. iced's text input keeps its own copy while
  you're typing, and Cryptdeck can't wipe that copy.
- With the VeraCrypt CLI, mounting needs root. When Cryptdeck isn't running as root, it runs
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
| `CRYPTDECK_BACKEND=udisks`, `veracrypt` or `mock` | Force a backend (`mock` is an in-memory demo) |
| `CRYPTDECK_VERACRYPT=/path/to/veracrypt` | Use a specific `veracrypt` binary (it must be root-owned to mount) |
| `CRYPTDECK_THEME=light` or `dark` | Use light or dark for this run, whatever is picked in the app |
| `CRYPTDECK_LANG=de` or `en` | Override the system language |
| `CRYPTDECK_CONFIG_DIR=/path` | Keep the saved volume list somewhere else |

The saved volume list is `volumes.json` in your config folder:
`~/.config/cryptdeck` on Linux, `~/Library/Application Support/cryptdeck`
on macOS, and `%APPDATA%\cryptdeck` on Windows.

## Dependency checks

Every push, and a weekly scheduled run, checks the Rust dependencies with
[cargo-deny](https://embarkstudios.github.io/cargo-deny/) against
`deny.toml`: known vulnerabilities (RustSec), yanked crates, licenses, and
crate sources. Run it locally with `cargo deny check`. GitHub Actions are
pinned to commit hashes, which Dependabot keeps up to date.

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

The one-click workflow also adds the release to the Flatpak metadata
(`packaging/flatpak/io.github.m411s.Cryptdeck.metainfo.xml`). When bumping
by hand, add a `<release>` entry there too; the release build warns if
it's missing.

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
| `Cryptdeck-<version>-x86_64.flatpak` | Linux, Flatpak (freedesktop 25.08 runtime) |

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
- [ ] VeraCrypt CLI fallback: polkit helper with a restricted policy instead of a raw `pkexec veracrypt`
- [x] udisks2 backend (no VeraCrypt or root needed on Linux; volumes owned by you)
- [ ] Hidden volume protection
- [x] System tray (Linux)
- [x] Lock on sleep and screen lock (Linux)
- [x] Hidden volumes
- [x] German translation
- [x] Flatpak bundle in releases
- [ ] Flathub (needs the repository to be public)
- [ ] System tray (macOS, Windows)
- [ ] Developer ID signing and notarization (macOS), code signing (Windows)
- [ ] Volume creation wizard
- [ ] macOS
- [x] Windows: unlock, lock and list through VeraCrypt
- [ ] Windows: pass the password to the VeraCrypt driver directly, not on VeraCrypt.exe's command line

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Cryptdeck runs VeraCrypt as a separate program and contains no VeraCrypt
code, so VeraCrypt's licenses (Apache 2.0 and the TrueCrypt License 3.0) do
not apply to it.

The downloads also contain the open-source Rust crates Cryptdeck is built
from. Their licenses and notices are in `THIRD-PARTY-LICENSES.html`, which
comes with every download (in the AppImage and Flatpak under
`share/licenses/`, on macOS in `Cryptdeck.app/Contents/Resources`). It's
generated from `Cargo.lock` with
[cargo-about](https://embarkstudios.github.io/cargo-about/):

```bash
cargo install --locked cargo-about --features cli
cargo about generate --locked --fail packaging/about.hbs -o THIRD-PARTY-LICENSES.html
```

"VeraCrypt" is a name of IDRIX, used here only to say what Cryptdeck works
with. Cryptdeck is not affiliated with or endorsed by IDRIX or the
VeraCrypt project.
