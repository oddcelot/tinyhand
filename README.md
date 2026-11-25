<img src="./.github/logo.png" width="80" />

# TinyHand

[![Latest release](https://img.shields.io/github/v/release/nilbuild/tinyhand?display_name=tag&sort=semver&cacheSeconds=3600)](https://github.com/nilbuild/tinyhand/releases/latest)
[![Platforms: macOS, Windows, Linux](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey)](https://tinyhand.app)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](./LICENSE)

A tiny hand on a stick that points at things on your screen.

<img src="./.github/banner.jpg" alt="TinyHand: a tiny hand for your Mac" />

[![Download for macOS](https://img.shields.io/badge/Download_for_macOS-000000?style=for-the-badge&logo=apple&logoColor=white)](https://github.com/nilbuild/tinyhand/releases/latest/download/TinyHand.dmg)
[![Visit website](https://img.shields.io/badge/Visit_website-ffffff?style=for-the-badge&logo=safari&logoColor=black)](https://tinyhand.app)

macOS 13 or later (Apple Silicon and Intel). Previews for Ubuntu 24.04 and Windows.

## Install

**macOS:** open `TinyHand.dmg`, drag TinyHand to Applications and allow **Accessibility** when asked. It needs that to turn your clicks into taps. TinyHand updates itself.

**Ubuntu 24.04 (preview):** `sudo apt install ./tinyhand_<version>_amd64.deb`, then run `tinyhand` or open TinyHand from the app menu. On Wayland, apps can't watch the mouse or catch clicks for other apps, so while the pointer is on it covers the screen and takes every click; right-click or Esc turns it off. GNOME runs it through XWayland. Global shortcuts work on X11 and XWayland; elsewhere use the tray icon or bind a key to `tinyhand toggle`.

**Windows (preview):** run the installer from `packaging/windows/`. Not tested on a real PC yet.

## Use

| | |
|---|---|
| Click the menu bar (tray) icon, or **⌃⌥H** (Ctrl+Alt+H) | Turn the pointer on or off |
| Right-click the menu bar (tray) icon | Menu and Settings |
| Click | Tap (nothing underneath gets clicked) |
| Hold and drag | Draw on the screen |
| Hold **⌘** (Ctrl on Windows and Linux) while drawing | Add an arrowhead to the end of the line |
| **⌃⌥X** (Ctrl+Alt+X) | Clear the drawing |
| **Esc** | Clear everything and turn the pointer off |

While the pointer is on, a small toolbar at the bottom of the screen lets you pick one of
36 hands or a laser dot, mute the tap sound, or exit.

## Automation

Launchers like Raycast or Alfred, and shell scripts, can control TinyHand with
`tinyhand://` links. Use `open -g` so TinyHand doesn't take focus:

```sh
open -g tinyhand://toggle
```

| Link | Action |
|---|---|
| `tinyhand://on`, `tinyhand://off`, `tinyhand://toggle` | Turn the pointer on, off, or flip it |
| `tinyhand://clear` | Clear the drawing |
| `tinyhand://style/<id>` | Switch to a hand (or `laser`) and turn the pointer on |
| `tinyhand://sound/on`, `/off`, `/toggle` | Tap sound for the current pointer |

While TinyHand runs, it keeps its current state in
`~/Library/Application Support/TinyHand/state.json`: whether the pointer is on, the
selected style, whether sound is on, and every style with its `id`, name and type
(`hand`, `custom` or `laser`). The file is removed when TinyHand quits. It also
records the app's `pid`, so a file left behind by a crash can be detected.

If TinyHand isn't running, opening any link launches it and then runs the command.

On Windows and Linux, run the same commands with the `tinyhand` binary:
`tinyhand toggle`, `tinyhand style laser`, `tinyhand sound off`. They go to the running
copy. The state file is in the config folder (`~/.config/tinyhand/` on Linux,
`%APPDATA%\TinyHand\` on Windows).

## Build

TinyHand is written in Rust with [GPUI](https://www.gpui.rs). See
[crates/README.md](crates/README.md) for the layout, test hooks and packaging.

```sh
scripts/fetch-gpui.sh               # once: GPUI at the pinned commit, into vendor/zed
cargo run --release -p tinyhand     # run it
cargo test --workspace
scripts/build-app.sh                # macOS: dist/TinyHand.app
```

## License

MIT. See [LICENSE](LICENSE).
