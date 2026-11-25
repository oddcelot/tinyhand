# TinyHand in Rust

TinyHand for macOS, Windows and Linux, built on [GPUI](https://www.gpui.rs). It replaced
the original Swift app (macOS only); `crates/docs/swift-reference.md` records what that app did.

| Crate | What's in it |
|---|---|
| `core` (`tinyhand-core`) | No UI or OS code: measuring hand images, rotating them, the arm geometry, ink, settings. Unit-tested. |
| `platform` (`tinyhand-platform`) | Per-OS code: making the overlay window float and ignore clicks, reading the mouse. |
| `app` (`tinyhand`) | The GPUI app: windows and drawing. |

Packaging is in `packaging/`, the Linux test rigs in `tests/linux/`.

```sh
scripts/fetch-gpui.sh                           # once: Zed's GPUI at the pinned commit, into vendor/zed
cargo test --workspace                          # logic tests
cargo run --release -p tinyhand                 # tray icon; click it or ⌃⌥H to turn the pointer on
cargo run --release -p tinyhand -- style lavender   # commands go to the running copy
```

The app starts as a tray icon with the pointer off. Esc, the tray or the toggle shortcut
turn it off; on Linux right-click does too. Quit from the tray menu. Commands
(`tinyhand on|off|toggle|clear|style <id>|sound on|off|toggle`, or a `tinyhand://` URL)
are sent to the running copy over a localhost socket. Needs Rust 1.100
(`rust-toolchain.toml` asks rustup for it).

For unattended runs: `TH_ACTIVATE=1` turns the pointer on at launch, `TH_FAKE_MOUSE=x,y`
pins the pointer (and never starts the input tap), `TH_TAP_EVERY=seconds` taps on a timer,
`TH_SECONDS=n` quits after n seconds, `TH_DEBUG=1` logs every frame, `TH_CONFIG_DIR=dir`
keeps settings and `state.json` away from the real ones. `TH_INPUT=capture` uses the Linux
way of reading the mouse on macOS or Windows (careful: the overlay then swallows every
click until it is turned off).

## Packaging

### macOS: `dist/TinyHand.app`

```sh
scripts/build-app.sh                  # this Mac's architecture, ad hoc signed
scripts/build-app.sh --universal      # arm64 + x86_64 (adds the rustup target), lipo
CODESIGN_IDENTITY="Developer ID Application: …" scripts/build-app.sh
scripts/release.sh [--skip-notarize]  # universal, update keys, notarise, DMG, appcast
```

`scripts/lib.sh` `make_bundle` lays out the bundle: hand images, tap sounds and menu bar
icons in `Contents/Resources` (where `assets.rs` looks first next to the binary),
`tinyhand.png` for the welcome card, `AppIcon.icns`/`Assets.car`, `LSUIElement`, the
`tinyhand` URL scheme, Sparkle in `Contents/Frameworks`.

- **Bundle id `com.kamranahmed.tinyhand`**, the same as the Swift app had, so Sparkle
  updates an installed Swift copy in place. Settings move to `settings.json`; on its
  first run the app imports the Swift app's UserDefaults. Custom hands stay in
  `~/Library/Application Support/TinyHand/Hands`.
- **`tinyhand://` URLs**: GPUI's `on_open_urls` hands them to the controller, both at
  launch and while running. A second launch with a URL argument forwards it to the
  running copy (`instance.rs`).
- **Sparkle**: not linked. When the bundle's Info.plist has `SUFeedURL` (only
  `--with-updates`, which `release.sh` passes) `platform::updater` loads
  `Contents/Frameworks/Sparkle.framework` at runtime and starts
  `SPUStandardUpdaterController`; then the tray shows "Check for Updates…", About has the
  button and General the "Check for updates automatically" switch. Ad hoc builds are
  signed without hardened runtime: library validation refuses an ad hoc Sparkle in an
  ad hoc app ("different Team IDs").

### Ubuntu: `dist/tinyhand_1.1.0-1_amd64.deb`

Built in Docker with `tests/linux/ubuntu/build-deb.sh` (`cargo deb`, metadata in
`crates/app/Cargo.toml`), tested by `tests/linux/ubuntu/run-tests.sh`; see
`tests/linux/ubuntu/README.md`. Installs `/usr/bin/tinyhand`, hand images, tap sounds and
tray icons under `/usr/share/tinyhand/`, a desktop entry and the icon.

```sh
sudo apt install ./tinyhand_1.1.0-1_amd64.deb
tinyhand                     # or "TinyHand" in the app menu
```

- x86-64, built on Ubuntu 24.04: runs on 24.04 and newer, not on 22.04 (glibc 2.39).
- Stock Ubuntu (GNOME) has no layer shell, so it runs through XWayland there.
- Tested by installing on a clean `ubuntu:24.04` container and running under Sway,
  Xvfb (X11) and GNOME's compositor (mutter 46, headless). Not yet on a real Ubuntu
  desktop; on GNOME, transparency couldn't be checked headless.
- No `tinyhand://` handler is registered on Linux; use the `tinyhand` command.

### Windows

`packaging/windows/`: an Inno Setup script (per-user install, `tinyhand://` in HKCU) and
the plan. Not run on Windows yet. CI (`.github/workflows/rust.yml`) builds and tests on
macOS, Ubuntu 24.04 and Windows and makes the bundle and the installer.

## Two ways to get the mouse

**Watch (macOS, Windows).** The overlay ignores the mouse entirely and reads position and
button with native calls. Clicks still reach the app underneath; swallowing them (the
"tap") would need an event tap on macOS or a low-level mouse hook on Windows.

**Capture (Linux).** The overlay covers the screen and takes the mouse, so GPUI's normal
mouse events say where it is. Wayland doesn't let an app read the pointer anywhere else,
so this is the only way there. It also swallows clicks, which is what TinyHand's tap does
anyway. The keyboard is left alone, so Ctrl+C in the terminal still works.

- Wayland: a layer-shell surface on the overlay layer, stretched over the output.
  Works on compositors with `wlr-layer-shell`: KDE, Hyprland, Sway, niri, COSMIC and
  other wlroots ones. **Not GNOME**, which doesn't offer layer shell.
- X11: a screen-sized window; the window manager is asked to keep it above everything and
  on every workspace (EWMH). Transparency needs a compositor, as on any X11 desktop.

- GNOME (stock Ubuntu): no layer shell, so the app notices at startup and runs as an X11
  client through XWayland, which every GNOME session has. `TH_BACKEND=x11` forces this.

Right-click or Esc quits (Esc once the overlay has been clicked, where the system lets it
take the keyboard).

What capture mode doesn't do yet: let clicks through on demand (TinyHand's ⌥ and
double-⇧), hide the system cursor (GPUI has no hidden cursor style), more than one output.

## What it showed

**macOS works.** Hand and stick follow the mouse with the geometry of
`ArmView.swift`; the window is transparent, ignores clicks, sits above other apps and on
every Space. About 5–6% CPU and 80 MB.

**GPUI covers the drawing only.** Window level, click-through, Spaces, Dock icon and
reading the mouse in watch mode are native code in `crates/platform`.

**GPUI can't rotate raster images.** Only SVG icons take a transform. `crates/core/src/hand.rs`
rotates the hand on the CPU and keeps one image per half degree.

**Windows: written, not run.** `crates/platform` makes the window layered, topmost and (in
watch mode) transparent to clicks, and reads the cursor with `GetCursorPos`. It
type-checks for `x86_64-pc-windows-msvc`; the whole crate can't be built for Windows from
a Mac. Open question: does GPUI's DirectComposition window still draw once it is layered?

**Linux works, in a headless rig.** Under Sway (Wayland, layer shell) and Xvfb with
xcompmgr (X11), both in Docker on software rendering, the overlay draws transparently
over other apps, the fingertip follows the pointer, and a click lands on the overlay. See `tests/linux/`. Not tried on a real desktop yet, nor on KDE,
Hyprland or a real X11 window manager.

Two things the Linux run turned up:
- A click can go down and up between two frames, so the press is latched until the next
  frame reads it.
- GPUI holds a window without focus to ~30 fps; the overlay never has focus, so it turns
  that limit off (`inactive_frame_interval: None`).

## Why Zed's main branch

The crates.io release of GPUI (0.2.2) has no layer-shell windows. Zed's main branch does
(`WindowKind::LayerShell`), so the spike uses it at a pinned commit. It is fetched by
`scripts/fetch-gpui.sh` rather than a Cargo git dependency because the Zed repository contains two
packages named `gpui` and Cargo picks the wrong one. The few dependency patches Zed's
workspace applies are repeated in `Cargo.toml`.

## Build notes

- `gpui_platform` uses the `runtime_shaders` feature: compiling the Metal shaders at
  build time failed with this Xcode.
- `target/` reaches several GB.
