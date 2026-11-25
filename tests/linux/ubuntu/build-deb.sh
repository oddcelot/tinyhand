#!/usr/bin/env bash
# Run from the repo root on the host. Produces dist/tinyhand_*.deb
set -e
docker run --rm --platform linux/amd64 -v "$PWD":/work -w /work \
  -v ~/.cargo/registry:/cargo/registry -v ~/.cargo/git:/cargo/git \
  -v tinyhand-amd64-target:/target -e CARGO_HOME=/cargo -e CARGO_TARGET_DIR=/target \
  tinyhand-ubuntu-build bash -c '
    export PATH=/opt/cargo/bin:$PATH RUSTUP_HOME=/opt/rustup
    cargo build --release -p tinyhand
    (cd tests/linux/vpointer && cargo build --release)
    # cargo-deb finds the binary through CARGO_TARGET_DIR. Never touch ./target here:
    # it is the host'"'"'s own (macOS) build folder, mounted in.
    cargo deb -p tinyhand --no-build -o dist/
    cp /target/release/vpointer tests/linux/ubuntu/vpointer'
