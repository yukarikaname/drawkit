#!/bin/sh
# Format every language with the tool its platform recommends.
#
# Rust uses rustfmt, Kotlin ktlint with the IntelliJ code style in .editorconfig, Swift Apple's
# swift-format with .swift-format, and C# the .NET SDK's dotnet format with the same .editorconfig.
# CI runs the same tools in check mode, so a local run here is exactly what CI verifies.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

cargo fmt --all

# ktlint's standalone jar lives on Maven Central, which is far steadier than the release CDN.
ktlint=${KTLINT_JAR:-$root/.cache/ktlint-cli-1.8.0-all.jar}
if [ ! -f "$ktlint" ]; then
    mkdir -p "$(dirname "$ktlint")"
    curl -sSLo "$ktlint" \
        https://repo1.maven.org/maven2/com/pinterest/ktlint/ktlint-cli/1.8.0/ktlint-cli-1.8.0-all.jar
fi
java -jar "$ktlint" --format "app-android/**/*.kt" "app-android/**/*.kts"

swift format --in-place --recursive --configuration .swift-format \
    app-macos/Sources app-ios/Sources

# The WinUI project only restores on Windows; elsewhere the C# is left to that platform's CI.
if [ "$(uname -s)" = "Windows_NT" ] || [ "$(uname -s)" = "MINGW64_NT" ]; then
    dotnet format app-windows/DrawKit.WinUI.csproj --verbosity quiet
fi
