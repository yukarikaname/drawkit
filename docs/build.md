# Building DrawKit

The shells share one Rust core. Everything below builds a shell against that core; the macOS
package links the release static libraries from `target/release`.

## Requirements

- Rust stable (edition 2024).
- macOS shell: Xcode 27 with the macOS 27 SDK; the package uses Swift tools 6.4.
- iPad shell: Xcode 27 and the iOS 27 SDK (the build script selects the simulator or device SDK).
- Android shell: JDK 17, the Android NDK, compile SDK 37 and an arm64 device or emulator.
- Windows shell: .NET 8 SDK, Windows App SDK and the MSVC Rust target.

## macOS

```sh
cargo test                       # core, formats, figure and GPU tests
make app                         # dist/DrawKit.app
open dist/DrawKit.app
```

`make run` builds and opens it. The bundle is ad-hoc signed and self-contained: the Rust code is
linked statically, so `dist/DrawKit.app` can be copied to another Mac.

For development, `swift run --package-path app-macos` starts the window from the Swift package.

## iPad

The iPad target is a flat iOS bundle built without an Xcode project:

```sh
./scripts/build-ios.sh simulator       # build/ios/DrawKit.app
xcrun simctl install booted build/ios/DrawKit.app
xcrun simctl launch booted dev.drawkit.ipados
```

`./scripts/build-ios.sh device` builds for arm64 iPhones and iPads; installing on hardware still
needs a provisioning profile.

## Android

```sh
./scripts/build-android.sh              # JNI library and debug APK
adb install -r app-android/app/build/outputs/apk/debug/app-debug.apk
```

The project uses the Compose template from the Android CLI. The Android Gradle plugin, compile SDK
and NDK are taken from the local SDK.

## Windows

```powershell
rustup target add x86_64-pc-windows-msvc
cargo build --release --target x86_64-pc-windows-msvc -p drawkit-core
New-Item -ItemType Directory -Force app-windows/native | Out-Null
Copy-Item target/x86_64-pc-windows-msvc/release/drawkit_core.dll app-windows/native/drawkit_core.dll
dotnet run --project app-windows/DrawKit.WinUI.csproj -p:Platform=x64
```

See `app-windows/README.md` for the ARM64 variant and the current limits.

## CI and release

`.github/workflows/ci.yml` runs Rust format, clippy and tests; the macOS Swift shell; the Android
APK; and the Windows WinUI build.

Pushing a `v*` tag runs `.github/workflows/release.yml` and publishes an arm64 macOS zip. Windows
and Linux packages return with their native shells; the old self-drawn prototype binary is gone.
The macOS release needs an Xcode image with the macOS 27 SDK.
