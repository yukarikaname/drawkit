#!/bin/sh
# Install the GTK4 shell outside Flatpak. The Flatpak manifest is the supported route; this is the
# same file set for a distribution package or a local /usr/local test.
#
#   PREFIX=$HOME/.local packaging/install.sh

set -eu
prefix=${PREFIX:-/usr}
root=$(cd "$(dirname "$0")/.." && pwd)

cargo build --manifest-path "$root/app-linux/Cargo.toml" --release

install -Dm755 "$root/app-linux/target/release/drawkit-linux" "$prefix/bin/drawkit"
install -Dm644 "$root/packaging/dev.drawkit.DrawKit.desktop" \
    "$prefix/share/applications/dev.drawkit.DrawKit.desktop"
install -Dm644 "$root/packaging/dev.drawkit.DrawKit.metainfo.xml" \
    "$prefix/share/metainfo/dev.drawkit.DrawKit.metainfo.xml"
install -Dm644 "$root/packaging/dev.drawkit.DrawKit.mime.xml" \
    "$prefix/share/mime/packages/dev.drawkit.DrawKit.xml"
install -Dm644 "$root/assets/drawkit.png" \
    "$prefix/share/icons/hicolor/512x512/apps/dev.drawkit.DrawKit.png"

# Refresh the caches when the tools are present; a package post-install does the same.
command -v update-desktop-database >/dev/null && update-desktop-database "$prefix/share/applications"
command -v update-mime-database >/dev/null && update-mime-database "$prefix/share/mime"
