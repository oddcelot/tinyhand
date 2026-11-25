# Linux test rig (headless Wayland / X11 in Docker)

Build the image (Debian trixie, arm64 on Apple Silicon, Rust 1.100.0, GPUI build deps,
Mesa lavapipe + llvmpipe, sway + grim + wlrctl, Xvfb + xdotool + ImageMagick):

    cd tests/linux && docker build -t tinyhand-linux-test .

Run from the repo root. The repo is mounted at /work, host cargo registry/git caches are
reused, and Linux build artifacts live in a named volume (kept apart from macOS `target/`):

    R=$PWD
    docker run --rm -it -v $R:/work -v ~/.cargo/registry:/cargo/registry -v ~/.cargo/git:/cargo/git \
      -v tinyhand-linux-target:/target -e CARGO_HOME=/cargo -e CARGO_TARGET_DIR=/target \
      tinyhand-linux-test bash

Inside (cwd is /work):

    cargo build --release -p tinyhand
    (cd tests/linux/vpointer && cargo build --release)   # Wayland test mouse, once
    tests/linux/run.sh wayland [app args]
    tests/linux/run.sh x11 [app args]

`run.sh` expects the binary at `$CARGO_TARGET_DIR/release/tinyhand` (override with
`APP=...`; `APP=none` only exercises the compositor and tools). Screenshots, app log and
compositor logs go to `tests/linux/out/`. Positions and timings are variables at the top
of `run.sh` (X1 Y1 X2 Y2 T_START T_MOVE T_CLICK T_SHOT), also overridable from the environment.

Wayland input comes from `vpointer/`, a small virtual mouse that stays plugged in for the
whole run, fed through a FIFO. `wlrctl` (still in the image) creates a virtual pointer per
command and removes it straight after, so a client that is already running never gets to
bind the seat's pointer and sees no mouse at all.

Rendering is software (Mesa lavapipe on 2 CPUs), so frame rates here say nothing about
real hardware.
