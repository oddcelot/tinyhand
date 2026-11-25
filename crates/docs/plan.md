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
| Sparkle updates | macOS: Sparkle loaded from the bundle; Windows/Linux: later |
| `scripts/lib.sh` bundle, sign, DMG, appcast | same scripts, pointed at the Rust binary |

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
2. **Logic and OS pieces**, in parallel: settings, styles and custom hands, ink model,
   hand maker in `core`; input tap, hotkeys, cursor, sound, login, tray in `platform`.
3. **App**: overlay with ink, laser dot, shadow and tap effect from settings; tray menu;
   hotkeys; Esc; double-⇧ click-through; ⌥ pass-through; automatic off on lock and sleep.
4. **Windows**: toolbar and ink tray; welcome card; settings window with every page;
   hand maker page.
5. **Shipping**: macOS bundle and Sparkle, `tinyhand://` URLs, `.deb`, Windows installer.
   Check on real machines, then remove the Swift app.
