#!/bin/sh
# Build DrawKit.app: the Rust static libraries, the Swift executable, its Info.plist and an ad-hoc
# signature. The bundle is what a person double-clicks; `swift run` starts a bare executable, which
# makes the menu bar and activation behaviour depend on the terminal that launched it.

set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-27.0}" cargo build --release -p drawkit-core -p drawkit-gpu
swift build --package-path app-macos -c release
bin="$root/app-macos/.build/release"

app="$root/dist/DrawKit.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$bin/DrawKit" "$app/Contents/MacOS/DrawKit"
cp app-macos/Resources/Info.plist "$app/Contents/Info.plist"
# The same table the catalog was generated from, as the .lproj folders Bundle.main looks up.
python3 scripts/gen-apple-strings.py
for folder in shared/apple/*.lproj; do
    cp -R "$folder" "$app/Contents/Resources/"
done
printf 'APPL????' > "$app/Contents/PkgInfo"

iconset=$(mktemp -d)/AppIcon.iconset
swift scripts/make-icon.swift "$iconset"
iconutil -c icns "$iconset" -o "$app/Contents/Resources/AppIcon.icns"
rm -rf "$(dirname "$iconset")"

# Ad-hoc signing is enough for a local build: it lets the window server treat the bundle as one app
# instead of a random binary. Developer ID signing belongs to the release job.
codesign --force --sign - "$app" >/dev/null 2>&1 || true

echo "built $app"
