#!/bin/sh
# Build the iPad shell. The Rust core is a static library; Swift is compiled directly against the
# iOS SDK and wrapped in a flat .app bundle. This keeps the iPad target independent of an Xcode
# project while the shell is still one screen.
#
# Usage: scripts/build-ios.sh [simulator|device]

set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
mode=${1:-simulator}
case "$mode" in
    simulator)
        rust_target=aarch64-apple-ios-sim
        sdk=$(xcrun --sdk iphonesimulator --show-sdk-path)
        triple=arm64-apple-ios27.0-simulator
        platform=iPhoneSimulator
        sdk_name=iphonesimulator
        ;;
    device)
        rust_target=aarch64-apple-ios
        sdk=$(xcrun --sdk iphoneos --show-sdk-path)
        triple=arm64-apple-ios27.0
        platform=iPhoneOS
        sdk_name=iphoneos
        ;;
    *)
        echo "usage: $0 [simulator|device]" >&2
        exit 2
        ;;
esac

cd "$root"
cargo build --release --target "$rust_target" -p drawkit-core -p drawkit-gpu

app="$root/build/ios/DrawKit.app"
rm -rf "$app"
mkdir -p "$app"
cp app-ios/Resources/Info.plist "$app/Info.plist"
/usr/libexec/PlistBuddy -c "Add :DTPlatformName string $platform" "$app/Info.plist"
/usr/libexec/PlistBuddy -c "Add :DTSDKName string $sdk_name" "$app/Info.plist"
/usr/libexec/PlistBuddy -c "Add :CFBundleSupportedPlatforms array" "$app/Info.plist"
/usr/libexec/PlistBuddy -c "Add :CFBundleSupportedPlatforms:0 string $platform" "$app/Info.plist"

iconset=$(mktemp -d)/AppIcon.iconset
swift scripts/make-icon.swift "$iconset"
for entry in "Icon-60@2x.png 120" "Icon-60@3x.png 180" "Icon-76.png 76" \
             "Icon-76@2x.png 152" "Icon-83.5@2x.png 167"; do
    set -- $entry
    sips -z "$2" "$2" "$iconset/icon_512x512@2x.png" --out "$app/$1" >/dev/null
done
rm -rf "$(dirname "$iconset")"

swiftc -sdk "$sdk" -Xcc -isysroot -Xcc "$sdk" -target "$triple" -parse-as-library -swift-version 6 -O \
    -L "$root/target/$rust_target/release" -ldrawkit_core -ldrawkit_gpu -liconv \
    -framework SwiftUI -framework UIKit -framework PencilKit \
    -framework Metal -framework IOSurface \
    -framework CoreGraphics -framework QuartzCore -framework Foundation \
    app-ios/Sources/DrawKit/*.swift shared/swift/*.swift -o "$app/DrawKit"

codesign --force --sign - "$app" >/dev/null 2>&1 || true
echo "built $app"
