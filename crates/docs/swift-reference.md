# Porting the Swift app to Rust

What the original Swift app (`Sources/TinyHand/`, removed when the Rust app replaced it;
see the git history before the commit "Replace the Swift app with the Rust app") did,
file by file. Written from the Swift code at the start of the port; the Rust code's
comments refer to these files. Numbers and key names are copied exactly.


## main.swift
**Test hooks (CLI flags; each one exits the process):**
- `--render-test [out=/tmp/armtest.png]` runs RenderTest.
- `--ax-status [out=/tmp/tinyhand-ax.txt]` writes `trusted=<AXIsProcessTrusted> tap=<CGEventTap creatable> bundle=<path>`.
- `--toolbar-test [out=/tmp/toolbar.png]` renders the toolbar with `.lavender` selected. Optional extras:
  - `--needs-permission`
  - `--hint` writes `*-hint.png` with the 4-part hint.
  - `--ink` writes `*-ink.png` showing the tray.
- `--settings-test [out=/tmp/settings.png] [section]` renders a settings page. `section` is one of `general|pointers|appearance|drawing|sound|about`. Env `TH_DARK` switches to dark mode.
- `--welcome-test [out=/tmp/welcome.png] [--untrusted]`
- `--make-hand <in> <out>` prints `ok %06X` (the ink colour) or `rejected: <message>` and exits 1.
- `--e2e-test` runs E2ETest.
- `--click-test` runs `OverlayController.runClickTest()`. It times tick, simulated down/up, and a 50-step drag, then prints the results.

---

## AppDelegate.swift
**What the user sees**
- **Status item.** Icon is `menubar_on` / `menubar_off` at 18×18 as a template image. The fallback is SF `hand.point.up.left(.fill)`.
  - In click-through mode the icon is tinted `systemOrange`.
  - **Left click** toggles the pointer. **Right click or ⌃-click** opens the menu.
  - Tooltips:
    - Off: `TinyHand — click to turn on, right-click for the menu`
    - On: `TinyHand is on — click to turn off, right-click for the menu`
    - Click-through: `TinyHand clicks go through — double-tap ⇧ to tap again, click to turn off`
- **Menu, in order:**
  1. `Allow Accessibility…`, hidden when trusted.
  2. `Turn Pointer On` or `Turn Pointer Off`, followed by 4 spaces and the toggle shortcut display.
  3. `Clear Drawing`, followed by 4 spaces and the clear shortcut display.
  4. `Tap Sound`, with a checkmark when on.
  5. Separator.
  6. `Settings…` (⌘,).
  7. `Check for Updates…`, only if Sparkle is available.
  8. Separator.
  9. `Quit TinyHand` (⌘Q).
- Settings, Welcome and Check for Updates all call `overlay.exit()` first, which clears the ink and deactivates.
- While the menu is open, `overlay.suspendInput(true)` disables the tap.
- The status item's window frame is added to the pass-through rects.
- **Main menu.** It exists only so text fields can edit: Close Window ⌘W, Quit ⌘Q, Undo ⌘Z, Redo ⇧⌘Z, Cut, Copy, Paste, Select All.
- **Launch flow:**
  1. Preload thumbnails.
  2. Enable background cursor hiding.
  3. Run the first-run login-item step.
  4. Start the updater.
  5. Register hotkeys.
  6. Show Welcome if `welcomeShown` is false **or** the app is not AX-trusted.
  7. Write `state.json`.
  8. Handle any URLs that arrived before launch finished. They are queued until then.
- **On terminate:** deactivate and delete `state.json`.
- **Changing a hotkey:** unregister both, save the new one, then re-register. If either registration fails, the old value is restored and the change reports failure.
- **While the settings recorder is recording,** both global hotkeys are unregistered.

**Persisted state**
- `loginItemSetUp` (Bool). On first run, if the bundle is in `/Applications/` or `~/Applications/`, the app sets this key and enables launch at login.

**OS APIs**
- NSStatusItem.
- `AXIsProcessTrusted`.
- Carbon hotkeys, through HotKeyCenter.
- `application(_:open:)` for the URL scheme.

---

## Preferences.swift
**UserDefaults keys** (names are copied exactly):

| key | type | default |
|---|---|---|
| `shortcut.toggle` | dict `{keyCode:Int, modifiers:Int (Carbon mask), display:String}` | ⌃⌥H: `kVK_ANSI_H`, `controlKey\|optionKey`, `"⌃⌥H"` |
| `shortcut.clear` | same | ⌃⌥X: `kVK_ANSI_X`, `"⌃⌥X"` |
| `pointerSize` | Double | `0.13` (range `0.08...0.20`, a fraction of screen height) |
| `showShadow` | Bool | `true` |
| `fadeSeconds` | Double | `3` (`0` means keep) |
| `sound.hands` | Bool | `true` |
| `sound.laser` | Bool | `false` |
| `tapSound` | String (TapSoundChoice) | `thock` |
| `tapEffect` | String | `fairyDust` |
| `interactKeepsHand` | Bool | `false` |
| `doubleShiftClickThrough` | Bool | `true` |
| `interactKey` | String `option\|command\|control\|off` | `option` |
| `inkChoice` | String | `match` |
| `showToolbar` | Bool | `true` |
| `showInkButton` | Bool | `true` |
| `pinnedStyles` | [String] | `PointerStyle.defaultPinned`. Duplicates are removed; if the result is empty, it falls back to the default. `maxPinned = 8`. |
| `pointerStyle` | String | `butterfly3d`. Legacy values are migrated: `"dot"`→laser, `"sunny"`→coral. |
| `welcomeShown` | Bool | — |
| `firstRunHintShown` | Bool | — |
| `loginItemSetUp` | Bool | — |

- **InteractKey titles:** `⌥ Option`, `⌘ Command`, `⌃ Control`, `Off`. Each maps to the CG flags `.maskAlternate`, `.maskCommand` and `.maskControl`.
- **InkChoice titles and colours:**
  - `Match the pointer` (no colour; uses the pointer's ink)
  - Red = `Ink.red`
  - Orange `0xFF8A00`
  - Yellow `0xFFCC00`
  - Green `0x30C25A`
  - Blue `0x0A84FF`
  - Purple `0x8754F2`
  - White `0xFFFFFF`
  - Black `0x1D1D21`
- **`launchAtLogin`** is not stored in UserDefaults. It reads and writes `SMAppService.mainApp` (register/unregister).
- **`resetAll()`** removes exactly these keys: `shortcut.toggle`, `shortcut.clear`, `pointerSize`, `showShadow`, `fadeSeconds`, `sound.hands`, `sound.laser`, `interactKey`, `interactKeepsHand`, `doubleShiftClickThrough`, `pinnedStyles`, `pointerStyle`, `inkChoice`, `showInkButton`, `showToolbar`, `tapSound`, `tapEffect`.
- **Every setter** posts `TinyHandPreferencesChanged`, which acts as a global change bus.

---

## HotKeyCenter.swift
- Wraps Carbon `RegisterEventHotKey` / `InstallEventHandler(kEventClassKeyboard, kEventHotKeyPressed)`.
- Signature is `0x54484B59` ("THKY") with incrementing ids.
- `register` returns nil if the key combination is already taken.
- It is used for the toggle hotkey, the clear hotkey and a dynamic **Esc** hotkey (keycode `kVK_Escape`, no modifiers).

---

## InputTap.swift
**OS API:** a `CGEvent.tapCreate(.cgSessionEventTap, .headInsertEventTap, .defaultTap)`. It runs its own `CFRunLoop` on a thread named `"TinyHand event tap"` at QoS `.userInteractive`, and is enabled or disabled with `tapEnable`.
- **Mask:** left/right/other mouse down, up and dragged.

**Behaviour while active:**
- **Timeouts.** `tapDisabledByTimeout` and `tapDisabledByUserInput` re-enable the tap.
- **Interact press.** This happens on a mouse down when the interact flag is set in the event flags or in `CGEventSource.flagsState(.hidSystemState)`, **or** when `clickThrough` is latched.
  - It marks an interact press and fires `onInteractDown`.
  - Until the button comes up, the interact modifier is **removed** from the event flags and the event passes through. This is ⌥ pass-through: the app gets a click without ⌥.
- **Pass-through rects** (CG coordinates). A down inside one passes through, and so does everything up to the matching up. It also fires `onPassThroughPress(location)`.
- **Locked screen.** `CGSessionCopyCurrentDictionary()["CGSSessionScreenIsLocked"]` is checked. If locked, downs and ups pass through and `onScreenLocked` fires.
- **Otherwise downs and ups are swallowed** (return nil) and `onMouseDown` / `onMouseUp` fire on the main thread.
- **Drags are rewritten** to `.mouseMoved` and passed on, so the cursor still moves without dragging anything. Left drags also fire `onLeftDragged`.
- **`maxWaitMs`** is a latency metric: the event timestamp compared with now, ignoring values of 600 000 ms or more.

---

## CursorHider.swift
**OS APIs:**
- `CGDisplayHideCursor` / `CGDisplayShowCursor(CGMainDisplayID())`.
- Private CGS calls via `dlsym`:
  - `CGSSetConnectionProperty(cid,cid,"SetsCursorInBackground",true)`, which lets an accessory app hide the cursor.
  - `CGSCurrentCursorSeed`, which detects when another app re-shows the cursor.

**Behaviour:**
- `show()` balances every hide call made so far (it tracks `hideCount`).
- `reassert(fighting:urgent:)` hides again when the cursor seed changed or an interval has passed. The interval is:
  - `0` if urgent
  - `0.03` if fighting (over the Dock)
  - `0.05` within 1 s of hiding
  - otherwise `1.0`
- `isSettling` is true for the first 1 s after hiding.

**DockArea:**
- Reads the Dock frame through AX: `com.apple.dock` → first child of `kAXChildren` → position and size, with a messaging timeout of `0.25`.
- It refreshes at most every 1 s on a utility queue.
- Hit-testing uses the frame inset by `-8` on each side.

**Port note.** This module is entirely macOS-private. Linux and Windows need their own cursor hiding (for example `ShowCursor` or an XFixes hide).

---

## OverlayController.swift (core state machine)
**Windows**
- Overlay: borderless, clear and non-activating. It covers the frame of the screen the mouse is on.
  - Level is `screenSaver+2`.
  - Collection behaviour: `canJoinAllSpaces, stationary, ignoresCycle, fullScreenAuxiliary`.
  - It ignores mouse events, except over the Dock as described below.
- The default frame is 1440×900 if no screen is found.

**Activate**
- Requires AX trust. Without it, `onNeedsPermission` opens Welcome.
- Then, in order:
  1. Warm the sound engine.
  2. Hide the cursor.
  3. Order the overlay front.
  4. Start a 120 Hz timer.
  5. Begin `ProcessInfo.beginActivity([.userInitiated,.latencyCritical], "TinyHand pointer is on")`.
  6. Install the tap and set the interact key; `clickThrough=false`.
  7. Install the ⇧ monitors.
  8. Place and show the toolbar; reveal it for 3 s.
  9. Enable Esc.
- **First run only** (key `firstRunHintShown`), a hint is shown for 10 s. Its parts are:
  - `Click` *to tap*
  - `Drag` *to draw*
  - `Hold <sym>` *to click for real*, only if the interact key is not Off
  - `Double-tap ⇧` *to click through*, only if that setting is on
  - `Esc` *to exit*

**Deactivate** stops the timer and monitors, disables the tap, clears ink and effects, hides the overlay and toolbar, unregisters Esc and shows the cursor.

**Esc** is registered only while active and only when no popup menu is open, our own menu is closed, and click-through is off. Esc calls `exit()`, which clears ink and deactivates.

**Automatic off.** The pointer turns off on any of these:
- Distributed notifications `com.apple.screenIsLocked` and `com.apple.screensaver.didstart`.
- `NSWorkspace` `screensDidSleep`, `willSleep` and `sessionDidResignActive`.
- A click on a locked screen.

On `activeSpaceDidChange`, if the overlay is not on the current space, it re-orders the overlay front and re-shows the toolbar.

**Double-⇧ click-through.** Uses global and local `NSEvent` `flagsChanged` monitors.
- A "tap" is ⇧ held **alone** for less than `0.3` s. The global input counter, the sum of `CGEventSource.counterForEventType(.hidSystemState)` for keyDown and left/right/other mouse down, must not change between ⇧ down and ⇧ up. This tells a clean tap apart from typing a capital letter.
- A double tap needs the second ⇧ down within `0.35` s of the previous tap, also with an unchanged counter. It toggles `clickThrough`.
- On toggle it shows a hint for 1.5 s:
  - On: `Clicks go through` *the hand stays*, `Double-tap ⇧` *to tap again*
  - Off: `Taps` *clicks don't reach apps*, `Double-tap ⇧` *to click through*
- The menubar icon turns orange while click-through is on.
- The same monitor also updates `ink.arrow` live while drawing.

**Mouse down (swallowed):**
1. `press()`, held=true.
2. Play the tap sound and hide the hint.
3. Record the down point and play the tap effect there.

**Drag:**
- A stroke begins only after **4 pt** of movement from the down point.
- Holding **⌘** (read from the HID flags) draws an arrowhead.

**Release** happens on mouse up, or from the tick loop when the HID left button is up after having been seen down. It ends the stroke.

**Tick** (120 Hz busy, 20 Hz idle):
- **Idle switch.** After `0.5` s with the mouse still, not held, not settling and not drawing, the timer drops to 20 Hz. It installs global and local monitors for move, drag, down and flagsChanged; any of these calls `wake()`.
- **Screen change.** If the mouse moves to another screen: release, clear ink and effects, move the overlay, re-place the toolbar, reveal it for 1.5 s.
- **Toolbar reveal zone:** `f.insetBy(dx:-80,dy:0).union(f.offsetBy(dx:0,dy:90)).insetBy(dx:0,dy:-10)`.
  - The toolbar is revealed if permission is needed, **or** if `showToolbar && !drawing && (near || within revealUntil)`.
- **Menu detection.** Popup menus are found with `CGWindowListCopyWindowInfo`, keeping windows whose layer equals `CGWindowLevelForKey(.popUpMenuWindow)`.
  - Scan interval is `0.05` s when a menu is likely, otherwise `0.25` s.
  - "Likely" means menus are already open, our own menu is open, a click hit the menu bar within the last 1 s, a button event happened within 2 s, or the mouse is within 40 pt below the menu bar.
  - Menu bar rect height is `max(24, frame.maxY − visibleFrame.maxY)`.
- **Real cursor** is shown, with the pointer hidden, when `!drawing && !interactKeepsHand && ((interactHeld && !clickThrough) || overMenu || ownMenuOpen)`.
- **Pass-through rects:**
  - the status item frame
  - the menu bar
  - open menus
  - the toolbar (and ink tray) when revealed
- **Over the Dock,** the overlay **stops ignoring mouse events** so the Dock never sees the hover, and the cursor hide is reasserted with "fighting" timing.
- **Permission poll.** Every 1 s, if the tap is not installed and the app is now trusted, it installs the tap and clears the toolbar's permission button.
- **Coordinates.** Cocoa uses bottom-left; CG uses top-left of the primary screen (`primaryH − y`).

**Sound split.** `soundOn` maps to `sound.laser` when the style is laser, otherwise to `sound.hands`.

---

## ArmView.swift (geometry; copy these values exactly)
**Constants:**
- `minRodFrac 0.05`
- `gripLeanFrac 0.30`
- `gripDepthFrac 0.12`
- `gripFollow 0.08`
- `liftFrac 0.06`
- `inkWidth = max(3, bounds.height*0.0055)`

**Layer order:** ink, then hand group (rod + hand), then laser dot, then effects.

**Grip point:**
- `targetX = clamp(cursor.x + 0.30*W, 0.15*W, 1.05*W)`
- Smoothing: `gx += (targetX−gx)*0.08` per tick.
- `grip = (gx, −0.12*H)`, which is below the bottom of the screen.

**Press:** `pressAmount = 1` on down. When not held it decays `pressAmount += (0−p)*0.18` per tick. `isSettling` is true if `p > 0.002` or `|target−gx| > 0.5`.

**Hand pose:**
- `handPts = size*H*(1−0.06*p)`
- `s = handPts/(wrist−handTop)`
- `widthPts = s*aspect`
- `handTipPts = (wrist−tip)*s`
- `tipOffX = (tipX−rodX)*widthPts`
- `along = sqrt(reach² − tipOffX²)`
- `rodLen = max(0.05*H, along − handTipPts)`
- `rotation = atan2(dy,dx) − atan2(rodLen+handTipPts, tipOffX)`
- The group's anchor is `(rodX, 0)` and it rotates about the grip.
- The rod layer uses the image band `[wrist..rodEnd]` and the hand layer uses `[handTop..wrist]`, both from the same PNG. The rod band is stretched to the needed length.

**Shadow:**
- `lift = 0.06*H*(1−0.8p)`
- Offset is `(0.6*lift, −lift)`, rotated by `−rotation`.
- Radius `max(1, lift*0.35) + 5p`.
- Opacity `0.26 + 0.02p`, or 0 if the shadow is off.
- Colour black.

**Laser dot:**
- `r = max(4, H*0.0038*size/0.13)*(1+0.2p)`
- Fill `Ink.red`, with a glow shadow of the same colour, opacity 1, radius `r*(2.2+1.0p)`.
- Core circle of radius `0.45r`, colour sRGB `(1, 0.86, 0.82, 0.95)`.

**Image detail:** the hand image is resampled to a height rounded up to a multiple of 64 px for the current backing scale. It uses the full image if the result is at least 90% of the original height.

**Tap-effect unit:** `H/900 * size/0.13`.

**Fallback:** if a style's asset fails to load, the style becomes `.laser`.

---

## Ink.swift
**Colours:**
- `purple` = sRGB(0.53, 0.33, 0.95)
- `red` = sRGB(1.0, 0.16, 0.12)

**Stroke:**
- Round cap and join.
- Shadow black, opacity 0.18, radius 1.5, offset (1, −1.5).
- A new point is added only if it is at least **1.5 pt** from the last one.
- Smoothing: quadratic curves through the midpoints, ending with a line to the last point.

**Arrowhead:**
- `size = max(14, lineWidth*3.2)`.
- The direction comes from the last point that is at least `size*1.2` back from the end.
- `head = min(size, len*0.8)`, spread `π/7`, drawn as two lines from the end.

**Ending a stroke:**
- The stroke is frozen into a rasterised layer.
- If `fadeSeconds > 0`, it fades out after `max(0.5, fadeSeconds)` over `0.6` s with ease-in, then is removed.
- `clear()` removes all strokes.

---

## PointerStyle.swift
**Built-in styles** (id, title, ink):

| id | title | ink |
|---|---|---|
| lavender | Lavender toy | purple |
| glove | Cursor glove | purple |
| coral | Coral vinyl | red |
| plush | Plush | 2F86F0 |
| robot | Robot | 16A87A |
| pink | Bubblegum | EE4F98 |
| blue | Sky | 2F86F0 |
| mint | Mint | 16A87A |
| lemon | Lemon | F2B705 |
| black | Midnight | 1D1D21 |
| chrome | Chrome | 6B7380 |
| holo | Holo | purple |
| fuzzy | Fuzzy | purple |
| wood | Wood | 9A5E2C |
| marble | Marble | 6B7380 |
| terrazzo | Terrazzo | F0561D |
| glass | Glass | 1597E5 |
| jelly | Jelly | A33FE6 |
| pastel | Pastel | EE4F98 |
| pixel | Pixel | 1D1D21 |
| lowpoly | Low poly | purple |
| knit | Knit | purple |
| skeleton | Skeleton | 9A5E2C |
| astro | Astronaut | 2F86F0 |
| space | Space | red |
| monster | Monster | 3E9E2E |
| water | Water | 1597E5 |
| lava | Lava | F0561D |
| grass | Grass | 3E9E2E |
| cactus | Cactus | 3E9E2E |
| choco | Chocolate | 9A5E2C |
| cloud | Cloud | 2F86F0 |
| mech | Mech | F0561D |
| neon | Neon | E0309A |
| leaf | Leaf | 3E9E2E |
| cookie | Gingerbread | 9A5E2C |
| pentacode | Pentacode | 0CA8D1 |
| butterfly | Butterfly | 1991C7 |
| butterfly3d | Butterfly 3D | 1991C7 |

- After the built-ins come the custom hands, then `laser` ("Laser dot", red).
- `defaultPinned = [butterfly3d, lavender, glove, coral, plush, robot, laser]`.
- Asset name is `hand_<id>` for built-ins and the raw id for custom hands.

**HandAsset.measure** (rows of alpha > 128 in the PNG):
- `top` = the first non-empty row.
- `maxW` = the widest row within `top..top+0.45h`.
- `fist` = the first row wider than `0.6*maxW`.
- `narrowing` = the first row from `fist` narrower than `0.35*maxW`.
- `stickW` = the median width of the rows from `narrowing` to the last row.
- `wrist` = the first row from `narrowing` with width ≤ `1.25*stickW`.
- `rodW` = the width at `wrist+h/50`.
- `rodEnd` = the first row from `wrist+h/33` wider than `1.8*rodW` (the handle), otherwise the last row.
- `handTop = (top−0.004h)/h`.
- `tipX` = the centre of row `top+h/200`.
- `rodX` = the centre of row `(wrist+rodEnd)/2`.
- `handMinX` / `handMaxX` come from rows `top..wrist`.

**Thumbnail:** a square of side `1.06*max(handW,handH)` centred on the hand, drawn at 128 px and shown as a 64 pt image.

**Cache:** LRU of 2 assets.

**Load order:**
1. `CustomHands.directory/<name>.png`
2. bundle
3. `exeDir/<name>.png`
4. `exeDir/assets/<name>.png`
5. `./assets/<name>.png`

---

## TapEffect.swift
**Choices:** `off` "Off", `fairyDust` "Fairy dust", `fairyDustSubtle` "Fairy dust, subtle". The emitter is a `CAEmitterLayer` with a circle shape.

**Timing:**
- Each burst emits for `0.12` s, then birth rate is set to 0.
- The layer is removed after `0.12 + longest lifetime + 0.1` s.
- Emitter radius is `4*unit`, or `3*unit` for subtle.

**Hues** (sRGB): (0.96, 0.30, 0.68), (0.60, 0.38, 0.98), (0.16, 0.62, 1.00), (0.10, 0.84, 0.70), (1.00, 0.70, 0.16).

**Particles** (every value below is multiplied by `unit`):
- **Ring:** ripple sprite of 128 px. Birth rate `1/burst`, life 0.5, scale 0.15, scaleSpeed 2.4, alphaSpeed −2.0. Not used in subtle mode.
- **For each hue:**
  - **Star** (sprite 112 px, four points, pinch 0.16): birth `3/burst`, life 1.1±0.4, velocity 120±75, yAccel −120, scale 0.42±0.2, scaleSpeed −0.24, spin 4, alphaSpeed −0.6.
  - **Dust bead** (sprite 56 px): birth `10/burst`, life 0.85±0.4, velocity 100±90, yAccel −130, scale 0.2±0.1, scaleSpeed −0.1, spin 3, alphaSpeed −0.8.
  - **Bubble** (bead sprite): birth `4/burst`, life 1.4±0.6, velocity 46±44, yAccel −30, scale 0.42±0.22, scaleSpeed −0.12, spin 2, alphaSpeed −0.55.
  - All three emit over the full circle (`emissionRange 2π`).
- **Subtle mode multipliers:**
  - birth ×0.3
  - velocity, velocity range and yAccel ×0.55
  - scale, scale range and scaleSpeed ×0.72
  - life ×0.75
  - `alphaSpeed = −0.7/(life+range)`
  - colour white at alpha 0.7
- **Sprites** are drawn procedurally: gradient glass stars and beads, plus an iridescent ring. The exact gradient stops are in `makeStar`, `makeBead` and `makeRipple` (lines 214–280).

---

## TapSound.swift
**Choices** (id → title):
- thock → Thock
- click → Click
- tock → Tock
- clack → Clack
- typewriter → Typewriter
- snap → Snap
- wood → Wood block
- pop → Pop
- boop → Boop
- blip → Blip
- coin → Coin
- pew → Pew
- squeak → Squeak
- quack → Quack
- chirp → Chirp

**Files:** `tap_<id>.wav`. Lookup order is bundle, `exeDir`, `exeDir/assets/sounds/`, then `./assets/sounds/`.

**Engine** (AVAudioEngine, all calls on a dedicated serial queue):
- **4** player nodes used round-robin.
- `maximumFramesToRender` is raised to `4096`.
- The engine is torn down after **30 s** idle.
- It is rebuilt when `AVAudioEngineConfigurationChange` fires, or when a scheduled buffer has not rendered after `duration+1` s.
- The fallback format is 44100 Hz mono.

---

## Toolbar.swift
**Panel:**
- Non-activating NSPanel, level `screenSaver+1`, same collection behaviour as the overlay.
- Dark glass: `.hudWindow` vibrant dark, tint white 0.07 at α 0.62, 1 px border white α 0.12, radius **16**.
- Stack spacing 6, insets 8/10/8/10.
- Placed centred at `visibleFrame.minY + 14`.

**Buttons:**
- Buttons are 40×40 with radius 10.
- Contents, in order:
  1. The pinned styles, plus the current style if it isn't pinned.
  2. A divider, 1×24, white α 0.18.
  3. The Ink button (`Ink colour`), hidden if `showInkButton` is false.
  4. Sound: `speaker.wave.2.fill` or `speaker.slash.fill`. Tooltips `Tap sound on — click to mute` and `Tap sound off — click to unmute`; tint white α 0.85 when on, 0.45 when off.
  5. Exit: `xmark`, tooltip `Exit  (Esc)`.
  6. The orange permission pill `  Allow clicks & drawing  ` with ⚠ (`systemOrange` α 0.9, black text, 12 pt semibold, height 32, radius 10). Its tooltip is `TinyHand needs Accessibility permission to intercept clicks and draw`.
- The selected style's highlight is white α 0.18.
- The laser icon is a 64×64 image: a red glow, a 14 pt red dot and a 6 pt core.

**Ink tray:**
- Glass with radius 14, spacing 2, insets 5/6.
- 9 swatches, each 30×30 with radius 15. "Match" is shown as a rainbow pie.
- Positioned above the ink button at `panel.maxY + 8`.
- Fades in and out over 0.12 s.
- Picking a swatch sets `inkChoice` and closes the tray.

**Ink dot image:** a 64 canvas with `r = 13`, a shadow α 0.35 offset (−1.5, −2.5), and a highlight ellipse.

**Hint:**
- Glass with radius 12, spacing 14, insets 9/16. It ignores the mouse.
- Positioned at `panel.maxY + 12`.
- Each part is a key in 12.5 pt semibold white, followed by text in 12.5 pt white α 0.7.
- Fades in over 0.2 s and out over 0.3 s.

**Reveal and hide:** fades in over 0.15 s and out over 0.35 s; mouse events are ignored while hidden.

---

## WelcomeWindow.swift
**Card:**
- Borderless, 340×300, `.popover` material, radius 20, movable by its background.
- Esc closes it.
- Contents: a 76 pt icon, a title at 19 pt semibold, a subtitle at 13 pt, a 180-wide default button (Return), a tip at 11 pt, and an `xmark` close button 14 pt in from the top-right corner.

**Two states,** refreshed every 0.5 s:
- **Untrusted:**
  - Title `Allow Accessibility`.
  - Subtitle `So your clicks become taps, not real clicks.`
  - Button `Open Settings`. It calls `AXIsProcessTrustedWithOptions(prompt:true)` and opens `x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility`.
  - Tip `Already on? Remove it and add it again.` appears 10 s after the button press.
- **Trusted:**
  - Title `You're all set`.
  - Subtitle `Press <toggle> any time to start.`
  - Button `Start`, which closes the card and activates the overlay.

Showing the card sets `welcomeShown = true`.

---

## SettingsWindow.swift + SettingsView.swift
**Window:**
- 780×560, minimum 760×540, transparent titlebar, title `TinyHand Settings`.
- Sidebar is 210 wide, with the `TinyHand` mark at the bottom.
- Sections and icon tiles:
  - General: `gearshape.fill`, grey 0.55
  - Pointers: `hand.point.up.left.fill`, (0.55, 0.42, 0.95)
  - Appearance: `sparkles`, (0.95, 0.45, 0.6)
  - Drawing: `pencil.tip`, (0.96, 0.58, 0.2)
  - Sound: `speaker.wave.2.fill`, (0.2, 0.55, 0.98)
  - About: `info.circle.fill`, grey 0.45

**Shortcut recorder:**
- Size 132×30, tooltip `Click, then press the new shortcut. Esc cancels.`
- Shows `Press keys` while recording.
- At least one of ⌃⌥⌘ is required; otherwise it shows `Add ⌃, ⌥ or ⌘`.
- Display order is ⌃⌥⇧⌘.
- Special key names: Space, ↩, ⇥, ⌫, ⌦, arrows, ↖ ↘ ⇞ ⇟, F1–F12. Other keys show the uppercased character.
- If registration fails the error reads `<display> is already used by another app. Pick a different shortcut.`
- Clicking outside the recorder cancels.

**General page:**
- **Permission card** (only if untrusted): `Accessibility`, *Needed so clicks become taps and you can draw.*, with an `Open Settings` button.
- **Shortcuts:** `Turn the pointer on or off` and `Clear the drawing`.
- **Clicking for real:**
  - `Hold to use the app`, *Clicks, drags and scrolls reach the app.*, an interactKey picker.
  - If the key is not Off: `While holding`, with options `Show the cursor` / `Keep the hand`. The detail text is *The normal cursor comes back, in menus too.* or *The hand stays, in menus too.*
  - `Double-tap ⇧ to click through`, a toggle. Detail: *Clicks reach the app and the hand stays. Double-tap again for taps. The menu bar icon turns orange.*
- **Startup:** `Open TinyHand when you log in`, and `Check for updates automatically` (only if Sparkle is available).

**Pointers page:**
- **"In your toolbar" row:** 8 slots, each 58 tall with radius 10, plus a `Reset` link.
  - Drag to reorder, with a 3 pt minimum distance and spring animation (0.25 / 0.85).
  - Drag outside the row (hit area inset −16/−28) to remove; a red minus badge shows.
  - The last pointer can't be removed: `Keep at least one pointer in the toolbar.`
  - Hover ✕ unpins.
  - A dashed `+` slot opens a 400-wide picker with 340-tall grid cells of 62×58. The picker lists all built-ins plus laser, then a "Your hands" section, and a `Make your own hand…` footer.
  - When the toolbar is full: `The toolbar is full. Remove one above to add <title>.`
  - Footer text:
    - Full: `The toolbar is full. Remove a pointer to add another. Drag to reorder.`
    - Otherwise: `Press + to add a pointer. Drag to reorder, or out to remove.`
  - The row is dimmed to 0.45 opacity if `showToolbar` is off.
- **Toolbar card:**
  - `Show the toolbar`. Detail is *Pops up when the pointer turns on and near the bottom of the screen.* or *Stays hidden. Press Esc to exit, or use the menu bar.*
  - `Ink button`, *Change the colour while you present.*
- **Your hands:**
  - `Make your own…` link.
  - Empty text: *Turn any idea into a hand with ChatGPT or another image tool.*
  - Chips: click to pin. If the toolbar is full, a swap popover opens: `The toolbar is full. Swap <t> in for:`. Hover ✕ deletes.
  - When a hand is deleted it is unpinned; if it was the saved style, the style falls back to `lavender`.

**Make your own hand** (sub-page, with a back link `‹ Pointers`):
1. **Describe it.** Field with placeholder `e.g. knitted wool in mustard yellow`.
2. **Make the image.**
   - `Open in ChatGPT` opens `https://chatgpt.com/?q=<prompt>`.
   - `Copy prompt` shows `Copied` for 1.6 s.
3. **Add it.**
   - A drop zone that also opens a file panel (PNG, WebP or image). Height 130, dashed `[5,4]` border.
   - Text `Drop the PNG here, or click to choose it`. On error it shows the error message and `Try again with another image`.
   - Preview: the draft cropped to (112, 90, 800, 1000), shown at 144×180 on a tan background (0.91, 0.81, 0.71).
   - `Name` field, at most 16 characters.
   - `Add hand` button and `Use another image` link.
   - On save: the hand is inserted before laser if fewer than 8 are pinned, it becomes the current style, and a message shows: `<t> is in your toolbar.` or `<t> is ready. Click it to add it to the toolbar.`

**Appearance page:**
- `Size` slider over 0.08–0.20, 180 wide.
- `Shadow under the pointer`.
- `Tap effect`, *Plays where the pointer taps.*, a picker.

**Drawing page:**
- `Ink colour`, 9 swatches of 20 pt. Detail is *Matches each pointer.* or *All pointers draw in <colour>.*
- `Lines fade`, with options `After 3 seconds`=3, `After 6 seconds`=6, `After 10 seconds`=10, `Keep until cleared`=0. Detail: *Clear them any time with <clear shortcut>.*

**Sound page:**
- Tiles in an adaptive 104–140 grid: `Off`, then the 15 sounds.
- Clicking a tile sets the sound, turns `sound.hands` on and previews it.
- Caption: *Click a sound to hear it. It plays each time the hand taps.*
- `Laser dot taps too`, *Off by default, for quiet pointing.* It is disabled when sound is Off.

**About page:**
- Version, *A tiny hand for your Mac. Free and open source.*, and a `Check for Updates` button.
- Links: Website https://tinyhand.app, Source https://github.com/nilbuild/tinyhand, Made by https://x.com/nilbuild, License MIT.
- `Reset all settings…` opens an alert:
  - Title `Reset all settings?`
  - Text: *Shortcuts, toolbar pointers, size, shadow, drawing and sound go back to how they were at first. Open at login and Accessibility stay as they are.*
  - Buttons Reset / Cancel. Reset also re-applies the default hotkeys.

**Snapshot env vars** (used by `--settings-test`):
- `TH_MAKE_HAND=<idea>` opens the hand-maker page.
- `TH_DRAFT=<png>` preloads a draft.
- `TH_SETTINGS_HEIGHT=<n>` sets the height and waits 4 s instead of 0.8 s.
- `TH_DARK` switches to dark mode.

---

## HandMaker.swift
**Prompt** (lines 4–20, copy it verbatim). The placeholder `[describe your hand, e.g. knitted wool in mustard yellow]` is replaced with the idea, or with `a soft vinyl toy in a bright colour` if empty.

**Validation** (messages copied exactly):
1. No alpha channel: `The image needs a transparent background. Save it as a PNG with transparency.`
2. Smaller than 64 px on either side: `The image is too small. Use one at least 512 pixels tall.`
3. More than 2% of edge pixels have α > 32: `The background isn't transparent, or the hand touches the edge of the image.`
4. Connected components on a grid with `step=max(1,min(w,h)/256)`, α > 128. The largest must be at least 97% of the total: `The image has more than one thing in it. Use a single hand.` (An empty image gives `The image is empty.`)
5. Bounding-box aspect h/w must be within 1.05–2.8: `The hand should stand upright, taller than it is wide.` The box height must be at least 50% of the image height: `The hand is too small in the image. Crop it closer.`
6. The top 8% band of the box must be at most 40% of the box width: `The top of the image should be a single finger pointing up.`
7. The bottom 4% span must be at least 20% of the box width: `The hand should end in a flat wrist or cuff at the bottom.`
8. Among pixels with α > 20 (sampled every 2 px), more than 8% with α < 200 gives: `The hand has a soft shadow or glow around it. Ask for no shadow.`

**Compose:**
- Canvas 1024×1536. The hand top is at y=150, hand height 560, `scale=min(560/bh, 700/bw)`.
- The wrist is centred horizontally.
- The rod:
  - width `clamp(bw*scale*0.18, 54, 76)`
  - from `handBottom−30` to y=1462
  - a capsule with a horizontal gradient `000000/121214/3A3A3E/121214/000000` at stops `0, .22, .45, .75, 1`

**Ink colour:**
- Sample every 3 px with α > 200 into a 6³ colour bucket.
- Score each bucket as `n*(0.25+sat)*(lum∈(0.12,0.85) ? 1 : 0.15)`.
- If the winning colour is near grey (max − min < 0.08), use (0.42, 0.45, 0.5).
- If its max channel is above 0.82, scale it to 0.82.

**Custom hands storage:**
- Directory: `~/Library/Application Support/TinyHand/Hands/`. Override with env `TH_HANDS_DIR`.
- Files: `<id>.png` plus `hands.json`.
- `hands.json` is a JSON array of `{"id":"custom-xxxxxxxx","name":String,"ink":UInt32}`. The id is the first 8 characters of a UUID, lowercased. `ink` is the decimal value of 0xRRGGBB.
- Default name is `My hand N`; names are at most 16 characters.
- Suggested name: the first two words of the filename, with `_` and `-` turned into spaces and the first letter capitalised. It falls back to the default name if the result is empty, longer than 16 characters, or starts with "chatgpt image".
- Changes post `TinyHandCustomHandsChanged`.

---

## Automation.swift
**URL scheme `tinyhand://`** (registered in Info.plist):
- `on`, `off`, `toggle`, `clear`
- `style/<id>`: picks the style **and activates**
- `sound/on|off|toggle` (no argument means toggle)

**State file** `~/Library/Application Support/TinyHand/state.json`:
- Pretty-printed with sorted keys.
- Fields: `{active, pid, soundOn, style, styles:[{id,name,type:"hand"|"custom"|"laser"}]}`.
- Rewritten when active changes, sound changes, or the custom hands list changes.
- Deleted on quit.

## Log.swift
Appends to `~/Library/Logs/TinyHand.log`. Each line is `yyyy-MM-dd HH:mm:ss.SSS [pid] msg`.

## Updater.swift
- Sparkle 2.10.0, `SPUStandardUpdaterController`.
- Only available when running from a `.app` whose Info.plist contains `SUFeedURL`.
- Gentle reminders are on, and the app activates before showing an update.
- Exposes `automaticallyChecks`.

## AppKitHelpers.swift
- `darkGlass(radius:)` (values listed under Toolbar).
- `roundCorners` as a stretchable mask image.
- Recursive `removeFocusRings`.

## RenderTest.swift
- One 420×560 panel per style, in a horizontal strip, on background (0.93, 0.94, 0.92).
- Target point `(190, 380)`.
- Draws a 61-step ellipse ink loop (rx 40, ry 32) centred at the target + (40, −8), then aims the pointer.
- Env:
  - `TH_RESTING`: not pressed, no ink.
  - `TH_NO_MARKER`: hides the yellow 6 px target dot.
- Exit codes: 3 (context), 4 (image), 5 (PNG).

## E2ETest.swift
Requires AX trust; it posts real events. Steps:
1. Opens a 520×360 catcher window that counts clicks and scrolls, and activates the overlay with lavender.
2. Posts a down, a 40-step sine drag and an up, and prints debugState.
3. Posts 5 scroll ticks and reports how many reached the app.
4. A plain click must add no strokes.
5. Clicks the toolbar's laser button; the style should become laser.
6. Freezes the main thread for 2 s while 3 clicks are posted, then reports tap latency (`CGGetEventTapList`), `tapMaxWaitMs` and leaked clicks.
7. Posts Esc (keycode 0x35) and checks that the overlay deactivated.
8. Restores the style and the mouse position.

---

## scripts/ and docs/
- `scripts/lib.sh`: shared constants (`BUNDLE_ID`, `REPO=nilbuild/tinyhand`, `FEED_URL=…/releases/latest/download/appcast.xml`, the Sparkle Ed public key, `SPARKLE_VERSION=2.10.0`).
  - `make_bundle` writes Info.plist with the `tinyhand` URL scheme and `LSUIElement`. With `--with-updates` it adds the `SU*` keys. It copies `hand_*.png`, `tap_*.wav`, `menubar_*.png`, the icons and Sparkle.framework.
  - `sign_app` codesigns the Sparkle parts, then the app.
  - `sparkle_tools` downloads Sparkle's `sign_update`.
- `scripts/build-app.sh`: local release build into `dist/TinyHand.app`, signed with `$CODESIGN_IDENTITY`, without update keys.
- `scripts/release.sh`:
  - Universal build (arm64 + x86_64), sign, notarise (`notarytool` with Apple ID env or a keychain profile, `--skip-notarize` to skip).
  - DMG via `create-dmg` (560×360 window).
  - Notarises the DMG and runs a Gatekeeper check.
  - Sparkle `sign_update` (`SPARKLE_PRIVATE_KEY` env or keychain account `tinyhand`).
  - Writes `appcast.xml` (build number `date +%Y%m%d%H%M`, notes from `RELEASE_NOTES`), a `.sha256`, and a copy named `TinyHand.dmg`.
- `scripts/make-icons.py`: SVG → PNG with rsvg-convert, then `actool` builds `Assets.car` and `AppIcon.icns` from `assets/AppIcon.icon`.
- `scripts/make-sounds.py`: synthesises 14 tap WAVs (44.1 kHz, mono, 16-bit) into `assets/sounds/`. **It has no `thock` generator, yet `tap_thock.wav` (the default sound) exists.** Its source is unknown and I'd treat it as a binary asset.
- `docs/`: static site for `tinyhand.app` (`CNAME`, `index.html`, `robots.txt`, `sitemap.xml`, `assets/` with hands, art, og image and `tap.wav`).
  - `docs/pointer.js` is a 564-line in-browser version of the pointer. Its `HANDS` table has per-hand pixel `tip`/`wrist` coordinates, stick gradient colours and ink. Some sticks are white, for example coral `#ffffff/#e7e3df/#c9c3bd`, while the Swift version uses the PNG's own rod band.

## Open questions
- **`interactKeepsHand` and click-through.** The real cursor only comes back if click-through is off. So when click-through is on, holding ⌥ keeps the hand even when `interactKeepsHand` is false. This looks intended.
- **⌥ in pass-through clicks.** The interact modifier is removed from both the mouse-down and mouse-up events. Apps therefore never see ⌥ on a pass-through click; ⌥-click as an app gesture is lost.
- **Hotkey modifiers on Linux/Windows.** Hotkeys are stored as Carbon keycodes and modifier masks. A cross-platform port needs a new key format, plus migration or a reset.