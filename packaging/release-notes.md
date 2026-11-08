## Downloads

On Linux, Cryptdeck uses your system's disk service (udisks2), so nothing
else is needed on most desktops. Otherwise it needs
[VeraCrypt](https://www.veracrypt.fr/en/Downloads.html) installed.

| Platform | File | Status |
| --- | --- | --- |
| Linux | `Cryptdeck-…-x86_64.AppImage` | Works |
| Linux | `cryptdeck-…-linux-x86_64.tar.gz` | Works (plain binary) |
| macOS (Apple Silicon and Intel) | `Cryptdeck-…-macos-universal.dmg` | Preview: the app runs, but can't unlock volumes yet |
| Windows | `Cryptdeck-…-windows-x86_64.zip` | Preview: the app runs, but can't unlock volumes yet |

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
