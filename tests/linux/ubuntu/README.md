# Ubuntu 24.04 x86-64 .deb build and clean-install test

Built on glibc 2.39, so the package runs on Ubuntu 24.04 and newer. All commands run on the
host from the repo root; Docker runs the amd64 images through Rosetta.

    docker build --platform linux/amd64 -f tests/linux/ubuntu/Dockerfile.build -t tinyhand-ubuntu-build tests/linux/ubuntu
    docker build --platform linux/amd64 -f tests/linux/ubuntu/Dockerfile.test  -t tinyhand-ubuntu-test  tests/linux/ubuntu
    tests/linux/ubuntu/build-deb.sh        # ~16 min cold; .deb lands in dist/
    tests/linux/ubuntu/run-tests.sh        # all three; or: run-tests.sh sway x11 gnome (any subset)

`build-deb.sh` mounts the repo at /work (and passes `-w /work`: an older image had another
WORKDIR), reuses host `~/.cargo/{registry,git}`, keeps the amd64 target dir in the volume
`tinyhand-amd64-target`, runs `cargo build --release`, builds `vpointer`, then
`cargo deb -p tinyhand --no-build` (cargo-deb honours CARGO_TARGET_DIR; the host's macOS
`target/` is not touched). `vpointer` (x86-64) is copied next to the scripts and is
build output.

`run-tests.sh` starts a clean ubuntu:24.04 container with only test tools, runs
`apt install /dist/tinyhand_*.deb`, then `scenarios.sh` drives sway (headless,
vpointer), Xvfb+xcompmgr (xdotool), and mutter (`dbus-run-session`, `mutter --headless
--wayland --virtual-monitor 1920x1080`). The app runs with `TH_ACTIVATE=1` (the pointer
on at launch; it is a tray app), `TH_DEBUG=1`, `TH_SECONDS=12` and a throwaway
`TH_CONFIG_DIR`, with a D-Bus session bus so the tray can start. The X11 case also sends
`tinyhand sound toggle` to the running copy and checks `state.json`. Output goes to `out/`;
each case prints frames, frames with the button down, whether the tray started and how
many hand/sound/tray images were missing.

## Results (2026-11-25)

- Package: `dist/tinyhand_1.1.0-1_amd64.deb`, 75 MB. Installs `/usr/bin/tinyhand`,
  `/usr/share/tinyhand/{hands/hand_*.png, sounds/tap_*.wav, menubar_*.png}`, the desktop
  entry and the 512 px icon. `Depends: libasound2t64, libc6 (>= 2.39), libvulkan1,
  libwayland-client0, libxcb1, libxkbcommon-x11-0, libxkbcommon0`; `Recommends:
  mesa-vulkan-drivers`. Installs cleanly on bare ubuntu:24.04.
- sway (layer shell): ~57 frames, a frame with the button down, butterfly, stick, fairy
  dust and the toolbar over a `foot` window (`sway-1/2/3.png`); tray started; no
  missing assets.
- X11 (Xvfb + xcompmgr): ~100 frames, hand and dust over an xterm (`x11-*.png`); tray
  started; `tinyhand sound toggle` from a second process flipped `soundOn` true → false.
  The toolbar does not show on X11 (only on layer shell).
- GNOME compositor (mutter 46 headless): `this Wayland compositor has no layer shell;
  using XWayland instead`, ~146 frames, hand and dust in `gnome-appwin.png` (root-window
  capture of mutter's XWayland fails, so that is the app window alone and its transparent
  area shows black); quits by itself at `TH_SECONDS`.
- No sound card in the container, so "no audio output" is expected. Software rendering
  (lavapipe) on 2 CPUs; frame counts say nothing about real hardware.
