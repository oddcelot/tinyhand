#!/usr/bin/env bash
# Run on the host from the repo root, after build-deb.sh. Installs the .deb in a clean Ubuntu 24.04
# amd64 container and runs the three scenarios. Output: tests/linux/ubuntu/out/
set -e
HERE=tests/linux
docker run --rm --platform linux/amd64 \
  -v "$PWD/dist":/dist:ro \
  -v "$PWD/$HERE/ubuntu":/t \
  -v "$PWD/$HERE/ubuntu/out":/out \
  tinyhand-ubuntu-test bash -c '
    set -e
    apt-get update -qq >/dev/null
    apt-get install -y /dist/tinyhand_*.deb 2>&1 | tee /out/apt-install.log | tail -40
    /t/scenarios.sh '"${*:-sway x11 gnome}"'
  ' 
