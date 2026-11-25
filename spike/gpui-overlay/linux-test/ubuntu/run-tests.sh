#!/usr/bin/env bash
# Run on the host from the repo root, after build-deb.sh. Installs the .deb in a clean Ubuntu 24.04
# amd64 container and runs the three scenarios. Output: linux-test/ubuntu/out/
set -e
HERE=spike/gpui-overlay/linux-test
docker run --rm --platform linux/amd64 \
  -v "$PWD/spike/gpui-overlay/dist":/dist:ro \
  -v "$PWD/$HERE/ubuntu":/t \
  -v "$PWD/$HERE/ubuntu/out":/out \
  tinyhand-ubuntu-test bash -c '
    set -e
    apt-get update -qq >/dev/null
    apt-get install -y /dist/tinyhand-preview_*.deb 2>&1 | tee /out/apt-install.log | tail -40
    /t/scenarios.sh '"${*:-sway x11 gnome}"'
  ' 
