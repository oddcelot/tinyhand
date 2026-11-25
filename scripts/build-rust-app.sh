#!/bin/bash
# Builds dist/rust/TinyHand.app from the Rust app (crates/app), with the same layout as
# the Swift app's bundle (scripts/build-app.sh): hand images, sounds and menu bar icons in
# Contents/Resources, Sparkle in Contents/Frameworks.
#
#   scripts/build-rust-app.sh               this Mac's architecture
#   scripts/build-rust-app.sh --universal   arm64 + x86_64, joined with lipo
#   scripts/build-rust-app.sh --with-updates   writes SUFeedURL, so Sparkle starts
#
# Until the Rust app replaces the Swift one it uses its own bundle id
# (com.kamranahmed.tinyhand.rs, RUST_BUNDLE_ID below), so it gets its own Accessibility
# permission and doesn't touch the installed Swift app. At cut-over, set
# RUST_BUNDLE_ID to "$BUNDLE_ID" (see crates/docs/plan.md).
#
# Signs with CODESIGN_IDENTITY, or ad hoc ("-") when that is unset.
set -euo pipefail
source "$(dirname "$0")/lib.sh"
cd "$ROOT"

RUST_BUNDLE_ID="com.kamranahmed.tinyhand.rs"
UNIVERSAL=false
UPDATES=""
OUT="dist/rust"
BUILD_NUMBER="0"
for arg in "$@"; do
    case "$arg" in
        --universal) UNIVERSAL=true ;;
        --with-updates) UPDATES="--with-updates" ;;
        --out=*) OUT="${arg#--out=}" ;;
        --build-number=*) BUILD_NUMBER="${arg#--build-number=}" ;;
        *) echo "usage: $0 [--universal] [--with-updates] [--out=dir] [--build-number=n]" >&2; exit 2 ;;
    esac
done

# The bundle's binary, built for this Mac or for both architectures.
if [[ "$UNIVERSAL" == true ]]; then
    HOST="$(rustc -vV | sed -n 's/^host: //p')"
    BINS=()
    for triple in aarch64-apple-darwin x86_64-apple-darwin; do
        if [[ "$triple" == "$HOST" ]]; then
            # The host build goes to target/release, which is usually built already.
            cargo build --release -p tinyhand
            BINS+=("target/release/tinyhand")
        else
            rustup target list --installed | grep -qx "$triple" || rustup target add "$triple"
            cargo build --release -p tinyhand --target "$triple"
            BINS+=("target/$triple/release/tinyhand")
        fi
    done
    BINARY="target/universal/tinyhand"
    mkdir -p "$(dirname "$BINARY")"
    lipo -create -output "$BINARY" "${BINS[@]}"
else
    cargo build --release -p tinyhand
    BINARY="target/release/tinyhand"
fi

APP="$OUT/$APP_NAME.app"
# Quit only a copy of this bundle that is running, never the installed Swift app.
pkill -f "$ROOT/$APP/Contents/MacOS/$APP_NAME" 2>/dev/null || true

mkdir -p "$OUT"
BUNDLE_ID="$RUST_BUNDLE_ID" make_bundle "$BINARY" "$APP" "$BUILD_NUMBER" $UPDATES
# The welcome card's icon (assets.rs looks for Resources/tinyhand.png).
cp packaging/linux/tinyhand.png "$APP/Contents/Resources/"
lipo -info "$APP/Contents/MacOS/$APP_NAME"

IDENTITY="${CODESIGN_IDENTITY:--}"
sign_app "$APP" "$IDENTITY" ${SIGN_TIMESTAMP:+--timestamp}
echo "Built $APP $VERSION ($RUST_BUNDLE_ID, signed with: $IDENTITY)"
