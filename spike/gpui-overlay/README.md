# GPUI overlay spike

Can TinyHand's pointer run on [GPUI](https://www.gpui.rs) so it works on Windows and Linux
too? This draws the hand on its stick on a click-through window above everything, follows
the mouse, and throws glass fairy dust when the left button goes down.

```sh
cargo run --release                 # Butterfly 3D
cargo run --release -- lavender     # any hand_<name>.png in ../../assets
```

Quit with Ctrl+C. No Accessibility permission is needed, because nothing is swallowed.

For unattended runs: `TH_FAKE_MOUSE=x,y` pins the pointer, `TH_TAP_EVERY=seconds` taps on
a timer, `TH_SECONDS=n` quits after n seconds.

## What it showed

**Works on macOS.** The hand and stick follow the mouse with the same geometry as
`ArmView.swift`, the fingertip lands on the cursor, the window is transparent, ignores
clicks, sits above other apps and on every Space. About 5–6% CPU and 80 MB while running.

**GPUI covers the drawing only.** Everything that makes it an overlay is native code in
`src/native.rs`: ignoring clicks, the window level, Spaces behaviour, hiding the Dock icon,
and reading the mouse (the window never receives mouse events, so GPUI's own mouse position
is useless here).

**GPUI can't rotate raster images.** Only SVG icons take a transform. `src/hand.rs`
rotates the hand on the CPU and keeps one image per half degree.

**Windows: written, not run.** `native.rs` makes the window layered, transparent to
clicks and topmost, and reads the cursor with `GetCursorPos`. That code type-checks for
`x86_64-pc-windows-msvc`, but the full crate can't be built for Windows from a Mac (a
GPUI dependency needs a Windows C toolchain), and it has never run. The open question is
whether GPUI's DirectComposition window still draws once it is made layered.

**Linux: not attempted.** On Wayland an app generally can't put a window above everything
or read the global mouse position, so it would need per-compositor work at best.

## Not in the spike

- Swallowing the click so the app underneath doesn't get it (TinyHand's "tap"). Needs an
  event tap on macOS and a low-level mouse hook on Windows.
- Drawing, shadow under the hand, sounds, toolbar, settings, menu bar icon, hotkeys,
  cursor hiding, multiple displays, idle frame pacing (it redraws every frame).

## Build notes

- `gpui` uses the `runtime_shaders` feature: compiling the Metal shaders at build time
  failed with this Xcode.
- First build downloads and compiles a lot; `target/` reaches several GB.
