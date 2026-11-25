# Ubuntu 24.04 x86-64 .deb build and clean-install test

Built on glibc 2.39, so the package runs on Ubuntu 24.04 and newer. All commands run on the
host from the repo root; Docker runs the amd64 images through Rosetta.

    docker build --platform linux/amd64 -f tests/linux/ubuntu/Dockerfile.build -t tinyhand-ubuntu-build tests/linux/ubuntu
    docker build --platform linux/amd64 -f tests/linux/ubuntu/Dockerfile.test  -t tinyhand-ubuntu-test  tests/linux/ubuntu
    tests/linux/ubuntu/build-deb.sh        # ~16 min cold; .deb lands in dist/
    tests/linux/ubuntu/run-tests.sh        # all three; or: run-tests.sh sway x11 gnome (any subset)

`build-deb.sh` mounts the repo at /work, reuses host `~/.cargo/{registry,git}`, keeps the
amd64 target dir in the volume `tinyhand-amd64-target`, runs `cargo build --release`, builds
`vpointer`, then `cargo deb -p tinyhand --no-build` (cargo-deb 3.8 honours CARGO_TARGET_DIR; the host's
macOS `target/` is not touched). `vpointer` (x86-64) is copied next to the scripts and is git-ignored-worthy build output.

`run-tests.sh` starts a clean ubuntu:24.04 container with only test tools, runs
`apt install /dist/tinyhand_*.deb`, then `scenarios.sh` drives sway (headless,
vpointer), Xvfb+xcompmgr (xdotool), and mutter (`dbus-run-session`, `mutter --headless
--wayland --virtual-monitor 1920x1080`). Output goes to `out/`.

## Results

- Package: `dist/tinyhand_0.1.0-1_amd64.deb`, 73 MB (29.7 MB binary + hand PNGs).
  `Depends: libc6 (>= 2.39), libxcb1 (>= 1.12), libxkbcommon-x11-0, libxkbcommon0`;
  `Recommends: mesa-vulkan-drivers`. The binary dlopens the Wayland, Vulkan and X libs.
- On bare ubuntu:24.04 `apt install ./x.deb` pulls (with Recommends) libvulkan1,
  libwayland-client0, libx11-6, libxcb-*, mesa-vulkan-drivers, libllvm20 ...; no unresolved
  `ldd` entries, no apt errors.
- sway (layer shell): 156 frames, `mouse down event at 700,450`, butterfly drawn over a
  `foot` window, fairy dust after the click (`sway-1/2/3.png`).
- X11 (Xvfb + xcompmgr): 202 frames, click logged, hand and dust over an xterm (`x11-*.png`).
- GNOME compositor (mutter 46.2 headless): log prints `this Wayland compositor has no layer
  shell; using XWayland instead`, then 340+ frames, click logged, mouse moved to 1198,600.
  Needs DISPLAY and XAUTHORITY of mutter's XWayland (a real GNOME session exports both; the
  script reads them from /tmp/.X11-unix and `$XDG_RUNTIME_DIR/.mutter-Xwaylandauth.*`) and a
  writable /tmp/.X11-unix. Root-window capture of mutter's XWayland fails, so
  `gnome-appwin.png` is a capture of the app window alone (transparent area shows black).
- Software rendering (lavapipe) on 2 CPUs; frame counts say nothing about real hardware.
