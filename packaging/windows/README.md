# TinyHand on Windows

Written, not yet run on a Windows machine. `crates/platform` has the Windows parts
(layered topmost overlay, `GetCursorPos`, low-level mouse hook, tray, `auto-launch` in
`HKCU\...\Run`); CI (`.github/workflows/rust.yml`, `windows-latest`) builds it, runs the
tests and makes the installer.

## Build

```powershell
bash scripts/fetch-gpui.sh                 # Git Bash; Zed's GPUI into vendor\zed
cargo build --release -p tinyhand
iscc /DVersion=1.1.0 packaging\windows\tinyhand.iss   # Inno Setup 6
```

The installer (`dist\windows\TinyHand-<version>-setup.exe`) is per user (no admin):
`%LOCALAPPDATA%\Programs\TinyHand\`, a Start menu entry, and the `tinyhand://` URL scheme
in `HKCU\Software\Classes\tinyhand` (Windows starts `tinyhand.exe "<url>"`, which hands the
URL to the running copy, see `crates/app/src/instance.rs`). Files go where
`crates/app/src/assets.rs` looks next to the exe: `assets\hand_*.png`,
`assets\menubar_*.png`, `assets\sounds\tap_*.wav`, `tinyhand.png`.

## Plan

1. **Run it on Windows 10 and 11**: overlay draws over other apps (open question: does
   GPUI's DirectComposition window still draw once it is layered?), clicks are swallowed
   by the mouse hook and let through with Alt / double-Shift, tray menu, Ctrl+Alt+H,
   settings window, `start tinyhand://sound/toggle`.
2. **Icon**: make `packaging/windows/tinyhand.ico` from `assets/AppIcon.icon` and embed it
   (`winresource` in a `build.rs`), then set `SetupIconFile` in `tinyhand.iss`.
3. **Signing**: sign `tinyhand.exe` and the installer (Authenticode, e.g. Azure Trusted
   Signing) so SmartScreen doesn't block the download.
4. **Uninstall**: also remove the `HKCU\...\Run` entry "Open at login" adds.
5. **Updates**: none yet (Sparkle is macOS only). Options: WinSparkle with the same
   appcast, or a check against GitHub releases that opens the download page.
6. **ARM64**: add `aarch64-pc-windows-msvc` once x64 works.
