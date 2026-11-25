# Plan: one Rust codebase for macOS, Windows and Linux

The Swift app (`Sources/TinyHand/`, described in `swift-reference.md`) is ported to the
Rust workspace in `crates/`. The Swift app stays the macOS release until the Rust app does
everything it does; then `Sources/`, `Package.swift` and the Swift build scripts go.

## Where things go

| Swift | Rust |
|---|---|
| ArmView geometry, HandAsset.measure, TapEffect particles | `core`: `arm`, `hand`, `particles` (done) |
| Preferences (UserDefaults) | `core::settings`: a JSON file in the config folder |
| PointerStyle catalogue, custom hands (`hands.json`) | `core::styles`, `core::custom_hands` |
| Ink strokes, arrowheads, fading | `core::ink` (model) + drawing in `app` |
| HandMaker (validate, compose, ink colour, prompt) | `core::hand_maker` |
| Automation `state.json`, `tinyhand://` commands | `core::automation` (parsing, state) + URL registration per OS |
| InputTap (CGEventTap) | `platform::input`: macOS event tap, Windows low-level mouse hook, Linux capture window |
| HotKeyCenter (Carbon) | `platform::hotkeys`: `global-hotkey` crate (macOS, Windows, X11) |
| CursorHider | `platform::cursor` per OS |
| TapSound (AVAudioEngine) | `platform::sound`: `rodio` |
| Launch at login (SMAppService) | `platform::login`: `auto-launch` |
| Status item and menu | `platform::tray`: NSStatusItem on macOS, Win32 tray on Windows, StatusNotifierItem (D-Bus) on Linux |
| Overlay, Toolbar, Settings, Welcome windows | `app`, in GPUI |
| Sparkle updates | `platform::updater`: Sparkle loaded from the bundle at runtime (macOS); stub elsewhere |
| `scripts/lib.sh` bundle, sign, DMG, appcast | `scripts/build-rust-app.sh`, `release.sh --rust`; `.deb` via `cargo deb`; Inno Setup on Windows |

## Decisions that differ from the Swift app

- **Settings** move from UserDefaults to `settings.json` (macOS `~/Library/Application
  Support/TinyHand/`, Linux `~/.config/tinyhand/`, Windows `%APPDATA%\TinyHand\`). Key
  names stay the same. On macOS the first run imports the old UserDefaults.
- **Shortcuts** are stored as text (`"ctrl+alt+h"`) instead of Carbon key codes, so they
  mean the same on every OS. The macOS import converts the old ones.
- **Custom hands** stay where they are on macOS; on Linux and Windows they go next to the
  settings file.

## Limits on Linux Wayland

Wayland doesn't let apps read the pointer, hide the cursor, register global shortcuts or
swallow clicks for other windows. The overlay takes the mouse itself (capture mode) on
layer-shell compositors and uses XWayland on GNOME. Global shortcuts work under X11 and
XWayland; on a pure Wayland session the tray menu and `tinyhand toggle` (for a compositor
key binding) are the way to turn it on.

## Phases

1. **Workspace** (done): `core`, `platform`, `app` crates; logic tests; Linux rigs moved.
2. **Logic and OS pieces** (done): settings, styles and custom hands, ink model, hand
   maker in `core`; input tap, hotkeys, cursor, sound, login, tray in `platform`.
3. **App** (done): overlay with ink, laser dot, shadow and tap effect from settings; tray
   menu; hotkeys; Esc; double-⇧ click-through; ⌥ pass-through; automatic off on lock and
   sleep.
4. **Windows** (done): toolbar and ink tray; welcome card; settings window with every
   page; hand maker page.
5. **Shipping** (done, apart from real-machine checks):
   - macOS: `scripts/build-rust-app.sh [--universal] [--with-updates]` →
     `dist/rust/TinyHand.app`, bundle id `com.kamranahmed.tinyhand.rs`;
     `release.sh --rust` for DMG, notarisation and appcast.
   - `tinyhand://` URLs through GPUI's `on_open_urls`, at launch and while running.
   - Sparkle loaded at runtime when Info.plist has `SUFeedURL`; tray "Check for
     Updates…", About button, General "Check for updates automatically".
   - Ubuntu `.deb` with hands, sounds and tray icons; Docker install test under Sway,
     X11 and GNOME (`tests/linux/ubuntu/`).
   - Windows: Inno Setup script and plan in `packaging/windows/`; CI for all three OSes
     in `.github/workflows/rust.yml`.

## What's left

1. **Real-machine checks.**
   - macOS: a signed, notarised `release.sh --rust --skip-notarize` dry run, then a real
     one; an actual Sparkle update from an older Rust build; Accessibility prompt and
     event tap on a clean Mac; Intel Mac (the universal build runs under Rosetta).
   - Ubuntu desktop on real hardware (GNOME on Wayland and X11, KDE): transparency, tray,
     shortcuts, sound.
   - Windows 10/11: everything (see `packaging/windows/README.md`); the app has only been
     type-checked for Windows and built in CI.
2. **Small gaps**: the workspace version in `Cargo.toml` is kept in step with `VERSION`
   by hand; the toolbar doesn't open on X11/XWayland (only on layer shell, seen in the Docker runs); no `.ico` for Windows; no `tinyhand://` handler on Linux (a desktop entry with
   `x-scheme-handler/tinyhand` would do it); updates on Windows and Linux.
3. **Cut-over: replace the Swift app.**
   1. In `scripts/build-rust-app.sh` set `RUST_BUNDLE_ID="$BUNDLE_ID"`
      (`com.kamranahmed.tinyhand`) so Sparkle updates the installed Swift app in place and
      it keeps its Accessibility permission (the binary name in the bundle stays
      `TinyHand`). Settings are imported from UserDefaults on first run.
   2. Make `release.sh` build Rust by default (drop `--rust` and the Swift branch),
      `build-app.sh` call `build-rust-app.sh`, and `.github/workflows/build.yml`/
      `release.yml` use the Rust build; keep the appcast URL and the Ed key.
   3. Bump the workspace version to the next release (newer than the last Swift
      `CFBundleVersion`, which is a timestamp, so the build number from `release.sh` is
      what counts).
   4. Remove `Sources/`, `Package.swift`, `Package.resolved` and the Swift-only bits of
      `scripts/lib.sh` (`quit_running_app` by process name; `sparkle_framework` then always
      downloads); move `crates/docs/swift-reference.md` to history or delete it.
   5. Update the root `README.md` (install, build, platforms) and `docs/` (the site:
      Linux and Windows downloads).
