#!/bin/sh
# Xcode Cloud clones the sources without Rust. Build the static libraries the two app targets
# link: host (macOS), iOS device and the arm64 simulator slice.
#
# The project's LIBRARY_SEARCH_PATHS point at target/release and
# target/aarch64-apple-ios*/release, so this script only needs to run before xcodebuild.

set -eu

repo="${CI_PRIMARY_REPOSITORY_PATH:-$(cd "$(dirname "$0")/.." && pwd)}"

if ! command -v cargo >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
        sh -s -- -y --profile minimal --default-toolchain stable
fi
# shellcheck disable=SC1091
. "$HOME/.cargo/env"

cd "$repo"
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
cargo build --release -p drawkit-core -p drawkit-gpu
cargo build --release --target aarch64-apple-ios -p drawkit-core -p drawkit-gpu
cargo build --release --target aarch64-apple-ios-sim -p drawkit-core -p drawkit-gpu
