#!/usr/bin/env bash
# Package a release build of Cryptdeck as an AppImage.
#
#   packaging/linux/build-appimage.sh <cryptdeck-binary> <version> [output-dir]
#
# Needs network access to fetch appimagetool and the AppImage runtime.
set -euo pipefail

bin=$1
version=$2
out=${3:-dist}
arch=${ARCH:-x86_64}

root=$(cd "$(dirname "$0")/../.." && pwd)
here="$root/packaging/linux"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

appdir="$work/Cryptdeck.AppDir"
mkdir -p "$appdir/usr/bin" \
         "$appdir/usr/share/applications" \
         "$appdir/usr/share/icons/hicolor/256x256/apps"

install -m 755 "$bin" "$appdir/usr/bin/cryptdeck"
install -m 755 "$here/AppRun" "$appdir/AppRun"
install -m 644 "$here/cryptdeck.desktop" "$appdir/usr/share/applications/cryptdeck.desktop"
install -m 644 "$here/cryptdeck.desktop" "$appdir/cryptdeck.desktop"
install -m 644 "$root/assets/cryptdeck.png" "$appdir/usr/share/icons/hicolor/256x256/apps/cryptdeck.png"
install -m 644 "$root/assets/cryptdeck.png" "$appdir/cryptdeck.png"
ln -s cryptdeck.png "$appdir/.DirIcon"

# fetch <file> <url> [sha256]
fetch() {
    # GitHub release downloads fail now and then (5xx), so retry.
    curl -fsSL --retry 5 --retry-delay 5 --retry-all-errors -o "$1" "$2"
    local actual
    actual=$(sha256sum "$1" | cut -d' ' -f1)
    # Logged so a known-good build can be pinned (see APPIMAGETOOL_SHA256).
    echo "Downloaded $2 (sha256 $actual)"
    if [ -n "${3:-}" ] && [ "$actual" != "$3" ]; then
        echo "error: checksum mismatch for $2: expected $3" >&2
        exit 1
    fi
}

# appimagetool and the AppImage runtime aren't packaged by distributions, so
# use the official builds. Set APPIMAGETOOL / APPIMAGE_RUNTIME to local copies
# to skip the downloads. To pin the downloads, set APPIMAGETOOL_URL and
# APPIMAGE_RUNTIME_URL to specific release files and APPIMAGETOOL_SHA256 and
# APPIMAGE_RUNTIME_SHA256 to their checksums; the build then fails if a file
# changes.
tool=${APPIMAGETOOL:-}
if [ -z "$tool" ]; then
    tool="$work/appimagetool"
    fetch "$tool" \
        "${APPIMAGETOOL_URL:-https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-${arch}.AppImage}" \
        "${APPIMAGETOOL_SHA256:-}"
    chmod +x "$tool"
fi
runtime=${APPIMAGE_RUNTIME:-}
if [ -z "$runtime" ]; then
    runtime="$work/runtime-${arch}"
    fetch "$runtime" \
        "${APPIMAGE_RUNTIME_URL:-https://github.com/AppImage/type2-runtime/releases/download/continuous/runtime-${arch}}" \
        "${APPIMAGE_RUNTIME_SHA256:-}"
fi

mkdir -p "$out"
# CI runners have no FUSE, so let the tool unpack itself instead of mounting.
APPIMAGE_EXTRACT_AND_RUN=1 ARCH="$arch" VERSION="$version" \
    "$tool" --no-appstream --runtime-file "$runtime" \
    "$appdir" "$out/Cryptdeck-${version}-${arch}.AppImage"

echo "Built $out/Cryptdeck-${version}-${arch}.AppImage"
