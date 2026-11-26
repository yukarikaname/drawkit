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
# The source plist leaves the minimum OS to Xcode's build settings; this flat bundle needs the
# number written out.
minimum=$(echo "$triple" | sed -E 's/.*ios([0-9.]+).*/\1/')
/usr/libexec/PlistBuddy -c "Set :MinimumOSVersion $minimum" "$app/Info.plist"

temp=$(mktemp -d)
iconset="$temp/AppIcon.iconset"
swift scripts/make-icon.swift "$iconset"
for entry in "Icon-60@2x.png 120" "Icon-60@3x.png 180" "Icon-76.png 76" \
             "Icon-76@2x.png 152" "Icon-83.5@2x.png 167"; do
    set -- $entry
    sips -z "$2" "$2" "$iconset/icon_512x512@2x.png" --out "$app/$1" >/dev/null
done

# The launch screen shares the icon's mark. A colour and an image asset let iOS show ink before
# SwiftUI starts, so the in-app animation does not begin after a white flash.
assets="$temp/LaunchAssets.xcassets"
mkdir -p "$assets/LaunchBackground.colorset" "$assets/LaunchLogo.imageset"
cat > "$assets/Contents.json" <<'EOF'
{"info":{"author":"xcode","version":1}}
EOF
cat > "$assets/LaunchBackground.colorset/Contents.json" <<'EOF'
{"colors":[{"color":{"color-space":"srgb","components":{"alpha":"1.000","blue":"0.090","green":"0.098","red":"0.106"}},"idiom":"universal"}],"info":{"author":"xcode","version":1}}
EOF
cat > "$assets/LaunchLogo.imageset/Contents.json" <<'EOF'
{"images":[{"filename":"LaunchLogo.png","idiom":"universal","scale":"1x"},{"filename":"LaunchLogo@2x.png","idiom":"universal","scale":"2x"},{"filename":"LaunchLogo@3x.png","idiom":"universal","scale":"3x"}],"info":{"author":"xcode","version":1}}
EOF
swift scripts/make-launch-logo.swift "$assets/LaunchLogo.imageset/LaunchLogo@3x.png"
sips -z 512 512 "$assets/LaunchLogo.imageset/LaunchLogo@3x.png" \
    --out "$assets/LaunchLogo.imageset/LaunchLogo@2x.png" >/dev/null
sips -z 256 256 "$assets/LaunchLogo.imageset/LaunchLogo@3x.png" \
    --out "$assets/LaunchLogo.imageset/LaunchLogo.png" >/dev/null
xcrun actool "$assets" --compile "$app" --platform "$sdk_name" \
    --minimum-deployment-target 27.0 --output-partial-info-plist "$temp/assets.plist" >/dev/null
rm -rf "$temp"

swiftc -sdk "$sdk" -Xcc -isysroot -Xcc "$sdk" -target "$triple" -parse-as-library -swift-version 6 -O \
    "$root/target/$rust_target/release/libdrawkit_core.a" \
    "$root/target/$rust_target/release/libdrawkit_gpu.a" -liconv \
    -framework SwiftUI -framework UIKit -framework PencilKit \
    -framework Metal -framework IOSurface \
    -framework CoreGraphics -framework QuartzCore -framework Foundation \
    $(find app-ios/Sources/DrawKit -name '*.swift' | sort) shared/swift/*.swift -o "$app/DrawKit"

codesign --force --sign - "$app" >/dev/null 2>&1 || true
echo "built $app"
