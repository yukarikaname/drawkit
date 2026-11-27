# The GTK4 shell's build machine. The package list is the one CI installs on ubuntu-24.04, so a
# compile that passes here is a compile that passes there; the difference is that this one runs on
# the same laptop the shell is being edited on.
#
# Built once by scripts/check-linux.sh and reused. It is an image, not the app: the repository is
# mounted at run time, and the target directory lives in a container-only path so a Linux build
# cannot overwrite the macOS artifacts beside it.

FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential pkg-config clang cmake curl ca-certificates \
        libfontconfig1-dev libfreetype6-dev \
        libxkbcommon-dev libxkbcommon-x11-dev \
        libwayland-dev wayland-protocols \
        libx11-dev libx11-xcb-dev libxcb1-dev libxcb-randr0-dev \
        libxcb-xfixes0-dev libxcb-shape0-dev libxcb-xkb-dev \
        libvulkan-dev mesa-vulkan-drivers libssl-dev libdrm-dev libzstd-dev \
        libegl1-mesa-dev libgles2-mesa-dev libdbus-1-dev libasound2-dev \
        libgtk-4-dev libadwaita-1-dev \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -m -u 1000 build
USER build
ENV PATH=/home/build/.cargo/bin:/usr/local/bin:/usr/bin:/bin
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal

WORKDIR /src
CMD ["bash"]
