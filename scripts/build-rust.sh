#!/bin/sh
# Build the Rust libraries the Xcode targets link. Xcode scheme pre-actions call this so a build
# from the IDE never links a stale archive. `ipad` also builds both iOS slices because one scheme
# serves a simulator run and a device / "Designed for iPad" run.

set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
# Match the deployment target the Swift targets link against; otherwise the cc-compiled
# tract/pulp objects are built for the SDK version and the linker warns on every member.
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-27.0}"
export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-27.0}"

cargo build --release -p drawkit-core -p drawkit-gpu

mode=${1:-mac}
if [ "$mode" = "ipad" ]; then
    rustup target add aarch64-apple-ios aarch64-apple-ios-sim >/dev/null 2>&1 || true
    cargo build --release --target aarch64-apple-ios -p drawkit-core -p drawkit-gpu
    cargo build --release --target aarch64-apple-ios-sim -p drawkit-core -p drawkit-gpu
fi
