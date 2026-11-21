#!/usr/bin/env bash
# Build Cryptdeck as a Flatpak and install it for the current user.
#
#   packaging/flatpak/build.sh           build and install
#   packaging/flatpak/build.sh --bundle  also write Cryptdeck.flatpak
#
# Needs: flatpak, flatpak-builder, python3 with aiohttp and tomlkit
# (e.g. `sudo apt install flatpak flatpak-builder python3-aiohttp
# python3-tomlkit`), and network access for the first build.
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
here="$root/packaging/flatpak"
app=io.github.m411s.Cryptdeck

# flatpak-cargo-generator turns Cargo.lock into a list of crate downloads,
# so the build itself can run offline. Pinned by commit and checksum.
gen_commit=9d86d336a39b9af38b86f114b6ce5820fe260f91
gen_sha256=0a2db6be87d75910facef28ab46d4d6460802e8419ab850d0caa6a364d26b380
gen="$here/.flatpak-cargo-generator.py"
if ! echo "$gen_sha256  $gen" | sha256sum -c --status - 2>/dev/null; then
    curl -fsSL --retry 5 -o "$gen" \
        "https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/$gen_commit/cargo/flatpak-cargo-generator.py"
    echo "$gen_sha256  $gen" | sha256sum -c -
fi
python3 "$gen" "$root/Cargo.lock" -o "$here/cargo-sources.json"

# License notices of the bundled crates, if cargo-about is installed
# (`cargo install --locked cargo-about --features cli`).
if command -v cargo-about >/dev/null; then
    (cd "$root" && cargo about generate --locked --fail packaging/about.hbs \
        -o THIRD-PARTY-LICENSES.html)
else
    echo "note: cargo-about not found, building without THIRD-PARTY-LICENSES.html" >&2
fi

flatpak remote-add --user --if-not-exists flathub \
    https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak-builder --user --install-deps-from=flathub --force-clean \
    --repo="$root/repo" --install "$root/build-dir" "$here/$app.yml"

if [ "${1:-}" = "--bundle" ]; then
    flatpak build-bundle "$root/repo" "$root/Cryptdeck.flatpak" "$app"
    echo "Wrote $root/Cryptdeck.flatpak"
fi
echo "Installed. Run it with: flatpak run $app"
