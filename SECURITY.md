# Security policy

Cryptdeck handles passwords for encrypted volumes and, on Linux, asks for
administrator rights through polkit. Security reports are very welcome.

## Reporting a vulnerability

Please **don't open a public issue**. Report it privately through GitHub
instead: open the repository's **Security** tab and choose **Report a
vulnerability** (or go to
<https://github.com/m411s/Cryptdeck/security/advisories/new>).

Please include:

- what an attacker can do, and what they need to start with (for example
  another program running as you, or physical access),
- the platform and backend (udisks2, VeraCrypt CLI, Windows),
- steps to reproduce, and the Cryptdeck version.

You'll get a first answer within a week. Once a fix is released, the
advisory is published, crediting you unless you'd rather not be named.

## Supported versions

Cryptdeck is an early prototype. Only the latest release gets security
fixes.

## Scope

Cryptdeck implements no cryptography itself; it drives udisks2/cryptsetup
or VeraCrypt. Problems in those tools belong with their projects. Problems
in how Cryptdeck passes passwords, keyfiles or root rights to them are in
scope. The known limitations listed under
[Security notes](README.md#security-notes) in the README (for example the
password on VeraCrypt.exe's command line on Windows) don't need a report.
