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

fetch() {
    # GitHub release downloads fail now and then (5xx), so retry.
    curl -fsSL --retry 5 --retry-delay 5 --retry-all-errors -o "$1" "$2"
}

# appimagetool and the AppImage runtime aren't packaged by distributions, so
# use the official builds. Set APPIMAGETOOL / APPIMAGE_RUNTIME to local copies
# to skip the downloads (or to pin versions).
tool=${APPIMAGETOOL:-}
if [ -z "$tool" ]; then
    tool="$work/appimagetool"
    fetch "$tool" \
        "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-${arch}.AppImage"
    chmod +x "$tool"
fi
runtime=${APPIMAGE_RUNTIME:-}
if [ -z "$runtime" ]; then
    runtime="$work/runtime-${arch}"
    fetch "$runtime" \
        "https://github.com/AppImage/type2-runtime/releases/download/continuous/runtime-${arch}"
fi

mkdir -p "$out"
# CI runners have no FUSE, so let the tool unpack itself instead of mounting.
APPIMAGE_EXTRACT_AND_RUN=1 ARCH="$arch" VERSION="$version" \
    "$tool" --no-appstream --runtime-file "$runtime" \
    "$appdir" "$out/Cryptdeck-${version}-${arch}.AppImage"

echo "Built $out/Cryptdeck-${version}-${arch}.AppImage"
