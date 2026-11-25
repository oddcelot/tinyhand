#!/bin/bash
# Fetches Zed's GPUI at the commit this spike is pinned to into vendor/zed.
#
# Not a Cargo git dependency: the Zed repository holds two packages named "gpui" (the
# real one and a lint test fixture), and Cargo picks the wrong one. Path dependencies
# name the right directory.
set -euo pipefail
cd "$(dirname "$0")/.."
REV=e84c6c61adb54bc6140a85b307d18cf970c24ab5
if [[ "$(git -C vendor/zed rev-parse HEAD 2>/dev/null)" == "$REV" ]]; then
    echo "vendor/zed already at $REV"
    exit 0
fi
mkdir -p vendor/zed
git -C vendor/zed init -q
git -C vendor/zed fetch -q --depth 1 https://github.com/zed-industries/zed "$REV"
git -C vendor/zed checkout -q --detach FETCH_HEAD
echo "vendor/zed at $(git -C vendor/zed rev-parse --short HEAD)"
