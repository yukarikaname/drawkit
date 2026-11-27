#!/bin/sh
# Build and run the GTK4 shell on a real Wayland machine, from the laptop.
#
# The dmabuf path cannot be checked on a Mac: it needs a Wayland compositor, a DRM render node and
# a Vulkan driver that exports dma-bufs. This copies the tree to the machine over ssh, builds the
# container image there and runs the shell inside it with the Wayland socket and /dev/dri passed
# through. `DRAWKIT_TEST_HOST` picks the machine; the default is the Steam Deck on the desk.
#
#   scripts/linux-on-device.sh              # build and open a window
#   scripts/linux-on-device.sh cargo build  # just build
#   WAYLAND_DEBUG=1 scripts/linux-on-device.sh   # trace the protocol (wants zwp_linux_dmabuf_v1)
#
# The first run downloads the image's packages on the device (a few minutes).

set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
host=${DRAWKIT_TEST_HOST:-deck@steamdeck.lan}
remote=${DRAWKIT_TEST_DIR:-drawkit}
key=${DRAWKIT_TEST_KEY:-$HOME/.ssh/id_ed25519_deck}
ssh_opts="-o BatchMode=yes -o ConnectTimeout=10 -i $key"
image=${DRAWKIT_LINUX_IMAGE:-drawkit-linux}
bench=${DRAWKIT_BENCH_POINTER:-}
offload=${DRAWKIT_NO_OFFLOAD:-}

if [ -z "${DRAWKIT_TEST_HOST:-}" ] && [ ! -f "$key" ]; then
    echo "no ssh key at $key; set DRAWKIT_TEST_KEY or DRAWKIT_TEST_HOST" >&2
    exit 1
fi

echo "copying to $host:$remote" >&2
# shellcheck disable=SC2086
rsync -a --delete -e "ssh $ssh_opts" \
    --exclude target --exclude .container --exclude build --exclude dist \
    --exclude training --exclude .git \
    "$root/" "$host:$remote/"

if [ "$#" -eq 0 ]; then
    set -- cargo run --release --manifest-path app-linux/Cargo.toml
fi

# shellcheck disable=SC2086
# keep-id makes the container run as the deck user, so the mounted tree stays writable by it, and
# the target directory lives outside the repository so a sync cannot delete the build cache.
ssh $ssh_opts "$host" "mkdir -p \$HOME/drawkit-target \$HOME/drawkit-cargo/registry \$HOME/drawkit-cargo/git && cd $remote && \
    (podman image inspect $image >/dev/null 2>&1 || podman build -t $image -f scripts/linux.Dockerfile .) && \
    podman run --rm --userns=keep-id --device /dev/dri --security-opt label=disable \
        --mount type=bind,src=\$HOME/drawkit-target,dst=/target \
        --mount type=bind,src=\$HOME/drawkit-cargo/registry,dst=/home/ubuntu/.cargo/registry \
        --mount type=bind,src=\$HOME/drawkit-cargo/git,dst=/home/ubuntu/.cargo/git \
        --env CARGO_TARGET_DIR=/target \
        --env XDG_RUNTIME_DIR=/run/user/1000 \
        --env WAYLAND_DISPLAY=\${WAYLAND_DISPLAY:-wayland-1} \
        --env CARGO_TERM_COLOR=always \
        --env DRAWKIT_BENCH_POINTER=$bench \
        --env DRAWKIT_NO_OFFLOAD=$offload \
        \${WAYLAND_DEBUG:+--env WAYLAND_DEBUG=1} \
        --mount type=bind,src=/run/user/1000,dst=/run/user/1000 \
        --mount type=bind,src=\$(pwd),dst=\$(pwd) \
        --workdir \$(pwd) \
        $image $*"
