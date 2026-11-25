# GPUI overlay spike

Can TinyHand's pointer run on [GPUI](https://www.gpui.rs) so it works on Windows and Linux
too? This draws the hand on its stick on a window above everything, follows the mouse,
and throws glass fairy dust when the left button goes down.

```sh
./fetch-gpui.sh                     # once: Zed's GPUI at the pinned commit, into vendor/zed
cargo run --release                 # Butterfly 3D
cargo run --release -- lavender     # any hand_<name>.png in ../../assets
```

Quit with Ctrl+C. Needs Rust 1.100 (`rust-toolchain.toml` asks rustup for it).

For unattended runs: `TH_FAKE_MOUSE=x,y` pins the pointer, `TH_TAP_EVERY=seconds` taps on
a timer, `TH_SECONDS=n` quits after n seconds, `TH_DEBUG=1` logs every frame.
`TH_INPUT=capture` uses the Linux way of reading the mouse on macOS or Windows (careful:
the overlay then swallows every click until it quits).

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

What capture mode doesn't do yet: let clicks through on demand (TinyHand's ⌥ and
double-⇧), hide the system cursor (GPUI has no hidden cursor style), more than one output.

## What it showed

**macOS works.** Hand, stick and dust follow the mouse with the geometry of
`ArmView.swift`; the window is transparent, ignores clicks, sits above other apps and on
every Space. About 5–6% CPU and 80 MB.

**GPUI covers the drawing only.** Window level, click-through, Spaces, Dock icon and
reading the mouse in watch mode are native code in `src/native.rs`.

**GPUI can't rotate raster images.** Only SVG icons take a transform. `src/hand.rs`
rotates the hand on the CPU and keeps one image per half degree.

**Windows: written, not run.** `native.rs` makes the window layered, topmost and (in
watch mode) transparent to clicks, and reads the cursor with `GetCursorPos`. It
type-checks for `x86_64-pc-windows-msvc`; the whole crate can't be built for Windows from
a Mac. Open question: does GPUI's DirectComposition window still draw once it is layered?

**Linux works, in a headless rig.** Under Sway (Wayland, layer shell) and Xvfb with
xcompmgr (X11), both in Docker on software rendering, the overlay draws transparently
over other apps, the fingertip follows the pointer, and a click lands on the overlay and
throws the dust. See `linux-test/`. Not tried on a real desktop yet, nor on KDE,
Hyprland or a real X11 window manager.

Two things the Linux run turned up:
- A click can go down and up between two frames, so the press is latched until the next
  frame reads it.
- GPUI holds a window without focus to ~30 fps; the overlay never has focus, so it turns
  that limit off (`inactive_frame_interval: None`).

## Why Zed's main branch

The crates.io release of GPUI (0.2.2) has no layer-shell windows. Zed's main branch does
(`WindowKind::LayerShell`), so the spike uses it at a pinned commit. It is fetched by
`fetch-gpui.sh` rather than a Cargo git dependency because the Zed repository contains two
packages named `gpui` and Cargo picks the wrong one. The few dependency patches Zed's
workspace applies are repeated in `Cargo.toml`.

## Build notes

- `gpui_platform` uses the `runtime_shaders` feature: compiling the Metal shaders at
  build time failed with this Xcode.
- `target/` reaches several GB.
