> **Early prototype, use at your own risk.** Provided "as is", without
> warranty, and with no responsibility for any data loss. Keep backups of
> your files and of each volume's header (VeraCrypt: Tools → Backup Volume
> Header). Not affiliated with or endorsed by IDRIX or the VeraCrypt project.

## Downloads

On Linux, Cryptdeck uses your system's disk service (udisks2), so VeraCrypt
doesn't need to be installed. On Ubuntu, Debian, Fedora and others, udisks2
ships with VeraCrypt detection switched off: Cryptdeck then shows a
**Turn on VeraCrypt support** button that fixes it once, with your
administrator password. On other systems, Cryptdeck needs
[VeraCrypt](https://www.veracrypt.fr/en/Downloads.html) installed.

| Platform | File | Status |
| --- | --- | --- |
| Linux | `Cryptdeck-…-x86_64.AppImage` | Works |
| Linux | `cryptdeck_…_amd64.deb` | Works (Debian, Ubuntu, Mint and similar; install with `sudo apt install ./cryptdeck_*_amd64.deb`) |
| Linux | `cryptdeck-…-linux-x86_64.tar.gz` | Works (plain binary) |
| Linux | `Cryptdeck-…-x86_64.flatpak` | Works (needs udisks2, and on some distributions a one-time setup command: see the README's "udisks2 setup"; install with `flatpak install --user ./Cryptdeck-*.flatpak`) |
| macOS (Apple Silicon and Intel) | `Cryptdeck-…-macos-universal.dmg` | Preview: the app runs, but can't unlock volumes yet |
| Windows | `Cryptdeck-…-windows-x86_64.zip` | New: needs VeraCrypt installed (see the README's security notes) |

**Linux AppImage:** make it executable, then run it:

```bash
chmod +x Cryptdeck-*.AppImage
./Cryptdeck-*.AppImage
```

The builds aren't signed with a paid developer certificate yet, so the first
launch shows a warning:

- **macOS:** right-click Cryptdeck.app, choose **Open**, then **Open** again.
- **Windows:** in the SmartScreen dialog, choose **More info**, then **Run anyway**.

Check a download against `SHA256SUMS.txt` with `sha256sum -c SHA256SUMS.txt --ignore-missing`.

Each download includes Cryptdeck's licenses (MIT or Apache 2.0) and
`THIRD-PARTY-LICENSES.html` with the licenses of the open-source crates it's
built from.
