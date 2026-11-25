#!/bin/sh
# Build the GTK4/libadwaita shell. The shell is a separate cargo workspace so a macOS checkout
# does not need GTK; CI and Linux developers run this script instead.

set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cargo build --manifest-path "$root/app-linux/Cargo.toml" --release
mkdir -p "$root/build/linux"
cp "$root/app-linux/target/release/drawkit-linux" "$root/build/linux/drawkit-linux"
echo "built $root/build/linux/drawkit-linux"
