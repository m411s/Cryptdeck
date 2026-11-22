# Privacy policy

*Last updated: 21 November 2026*

Cryptdeck is an open-source desktop app for VeraCrypt volumes. It's a
volunteer project, not a company, and it has no servers of its own.

## In short

- **Cryptdeck collects nothing.** No telemetry, analytics, crash reports,
  advertising, tracking or accounts.
- **Cryptdeck makes no network connections of its own.**
- **Cryptdeck never saves passwords, PIMs or keyfile contents.** They're
  only kept in memory to unlock a volume, and wiped once it's unlocked
  (with one limit, see "Security notes" in the README).
- **Everything Cryptdeck stores stays on your computer.**

## What Cryptdeck stores

In your config folder (`~/.config/cryptdeck` on Linux,
`~/Library/Application Support/cryptdeck` on macOS, `%APPDATA%\cryptdeck` on
Windows, or `CRYPTDECK_CONFIG_DIR`):

| File | What's in it |
| --- | --- |
| `volumes.json` | The locations of the volumes you added (their paths, nothing else). **Forget** removes a volume from it. |
| `settings.json` | Your settings: lock on sleep, lock on screen lock, light or dark appearance. |

Delete the folder to remove everything Cryptdeck has stored.

## Where your password goes

Cryptdeck hands your password to the tool that unlocks the volume, on your
own computer, and nowhere else:

| Backend | How |
| --- | --- |
| udisks2 (Linux) | Over the local system D-Bus to the udisks service. |
| VeraCrypt CLI (Linux, macOS) | On VeraCrypt's standard input. |
| Windows | On VeraCrypt.exe's command line, which other programs on the same PC can briefly see (see "Security notes" in the README). |

## When something leaves the app

Only when you ask for it:

- **Get VeraCrypt** opens VeraCrypt's download page
  (`https://www.veracrypt.fr/en/Downloads.html`) in your web browser. What
  that site collects is covered by its own policies.
- **Open** shows an unlocked volume in your file manager.

What udisks2, VeraCrypt, polkit and your operating system do is up to them.

## Changes

Changes to this policy are made in this file, so its history shows every
version.

## Contact

Questions: open an issue at <https://github.com/m411s/Cryptdeck/issues>.
For security problems, see [SECURITY.md](SECURITY.md).
