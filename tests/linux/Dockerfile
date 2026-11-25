FROM debian:trixie
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl git jq make cmake pkg-config gcc g++ clang lld llvm build-essential \
    libasound2-dev libfontconfig-dev libfreetype-dev libgit2-dev libglib2.0-dev libssl-dev \
    libva-dev libvulkan-dev libwayland-dev libx11-dev libx11-xcb-dev libxcb1-dev \
    libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxcb-xkb-dev libxkbcommon-dev \
    libxkbcommon-x11-dev libxrandr-dev libxi-dev libxcursor-dev libdrm-dev libgbm-dev \
    libegl-dev libzstd-dev libsqlite3-dev \
    mesa-vulkan-drivers libgl1-mesa-dri libegl1 libgles2 vulkan-tools mesa-utils-bin \
    sway grim wlrctl foot weston wayland-utils \
    xvfb xdotool xterm x11-apps x11-utils imagemagick openbox \
    dbus-x11 procps fonts-dejavu-core \
 && rm -rf /var/lib/apt/lists/*
ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:$PATH
# Zed pins 1.100.0 in rust-toolchain.toml; install at least that.
RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain 1.100.0
# Runtime defaults for the test rig; CARGO_HOME is overridden at run time to the host mounts.
RUN apt-get update && apt-get install -y --no-install-recommends xcompmgr x11-xserver-utils && rm -rf /var/lib/apt/lists/*
ENV CARGO_TARGET_DIR=/target XDG_RUNTIME_DIR=/tmp/xdg
WORKDIR /work
