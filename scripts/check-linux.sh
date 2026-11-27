#!/bin/sh
# Compile and test the Linux shell in a container, on the laptop the shell is being edited on.
#
# macOS cannot link GTK, Wayland or Vulkan, so without this a mistake in app-linux/ only shows up
# after a push. The image carries the same packages CI installs (scripts/linux.Dockerfile), the
# repository is mounted where it already is, and the target directory is container-only so a Linux
# build never overwrites the macOS artifacts next to it.
#
#   scripts/check-linux.sh                                  # build the GTK4 shell
#   scripts/check-linux.sh cargo test -p drawkit-core       # any cargo command
#   scripts/check-linux.sh cargo clippy -p drawkit-gpu --all-targets -- -D warnings
#
# Uses Apple's `container` CLI; Docker or Podman also work (DRAWKIT_CONTAINER=podman).

set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
image=${DRAWKIT_LINUX_IMAGE:-drawkit-linux}
runtime=${DRAWKIT_CONTAINER:-container}
if ! command -v "$runtime" >/dev/null 2>&1; then
    echo "no '$runtime' command; start the container system with 'container system start'" >&2
    exit 1
fi

# Apple's CLI keeps its images and machines behind a service; a build fails with "plugins are
# unavailable" until that service runs, so start it rather than asking a person to remember.
if [ "$runtime" = container ]; then
    if ! container system status >/dev/null 2>&1; then
        echo "starting the container system" >&2
        container system start >/dev/null
    fi
    inspect_image() { container image inspect "$image" >/dev/null 2>&1; }
    build_image() {
        container build --tag "$image" --file "$root/scripts/linux.Dockerfile" "$root"
    }
    mount() { printf 'type=bind,source=%s,target=%s' "$1" "$2"; }
    run=container
else
    inspect_image() { "$runtime" image inspect "$image" >/dev/null 2>&1; }
    build_image() { "$runtime" build -t "$image" -f "$root/scripts/linux.Dockerfile" "$root"; }
    mount() { printf '%s:%s' "$1" "$2"; }
    run=$runtime
fi

if ! inspect_image; then
    echo "building the $image image (first run only)" >&2
    build_image
fi

# Both caches live beside the repository so a rebuild is incremental across runs.
cache="$root/.container"
mkdir -p "$cache/target" "$cache/cargo" "$cache/home"

if [ "$#" -eq 0 ]; then
    set -- cargo build --manifest-path app-linux/Cargo.toml
fi

exec "$run" run --rm \
    --user "$(id -u):$(id -g)" \
    --mount "$(mount "$root" "$root")" \
    --mount "$(mount "$cache/target" /tmp/drawkit-target)" \
    --mount "$(mount "$cache/cargo" /tmp/drawkit-cargo)" \
    --mount "$(mount "$cache/home" /tmp/drawkit-home)" \
    --workdir "$root" \
    --env CARGO_TARGET_DIR=/tmp/drawkit-target \
    --env CARGO_HOME=/tmp/drawkit-cargo \
    --env HOME=/tmp/drawkit-home \
    --env CARGO_TERM_COLOR=always \
    "$image" "$@"
