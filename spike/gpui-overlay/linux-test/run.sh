#!/usr/bin/env bash
# Usage: run.sh <wayland|x11> [app args...]   (run inside the container)
# Override via env: APP, OUT, W, H, X1 Y1 (first move), X2 Y2 (second move),
# BG_CLIENT (command shown behind the app), TH_SECONDS, TH_DEBUG,
# T_START (wait after app launch), T_MOVE, T_CLICK, T_SHOT.
# Set APP=/path/to/other-binary or APP=none to test only the compositor/tools.
MODE=${1:-wayland}; shift || true
HERE=$(cd "$(dirname "$0")" && pwd)
APP=${APP:-${CARGO_TARGET_DIR:-/target}/release/tinyhand-overlay}
OUT=${OUT:-$HERE/out}
W=${W:-1920}; H=${H:-1080}
X1=${X1:-700}; Y1=${Y1:-450}; X2=${X2:-1200}; Y2=${Y2:-600}
T_START=${T_START:-2}      # let the app create its window
T_MOVE=${T_MOVE:-0.5}      # after first move, before click
T_CLICK=${T_CLICK:-0.3}    # after click, before shot 1
T_SHOT=${T_SHOT:-0.4}      # between shot 1 and shot 2
mkdir -p "$OUT" "$XDG_RUNTIME_DIR"; chmod 700 "$XDG_RUNTIME_DIR"
BG_TEXT_CMD='sh -c "seq 1 200; sleep 60"'
LOG=$OUT/$MODE-app.log; PIDS=()
cleanup() { for p in "${PIDS[@]}"; do kill "$p" 2>/dev/null; done; wait 2>/dev/null; }
trap cleanup EXIT

if [ "$MODE" = wayland ]; then
  export WLR_BACKENDS=headless WLR_RENDERER=${WLR_RENDERER:-pixman} WLR_LIBINPUT_NO_DEVICES=1
  cat > /tmp/sway.conf <<CONF
output HEADLESS-1 resolution ${W}x${H} position 0 0
default_border none
CONF
  sway -c /tmp/sway.conf >"$OUT/sway.log" 2>&1 & PIDS+=($!)
  for _ in $(seq 50); do ls "$XDG_RUNTIME_DIR"/wayland-* >/dev/null 2>&1 && break; sleep 0.2; done
  export WAYLAND_DISPLAY=$(basename "$(ls "$XDG_RUNTIME_DIR"/wayland-[0-9] | head -1)")
  export SWAYSOCK=$(ls "$XDG_RUNTIME_DIR"/sway-ipc.* | head -1)
  unset DISPLAY
  BG_CLIENT=${BG_CLIENT:-foot $BG_TEXT_CMD}
  # A virtual mouse that stays plugged in (wlrctl's comes and goes per command, so a
  # client that starts first never gets to bind the pointer). Fed through a FIFO.
  VPOINTER=${VPOINTER:-${CARGO_TARGET_DIR:-/target}/release/vpointer}
  rm -f /tmp/vpointer.fifo && mkfifo /tmp/vpointer.fifo
  W=$W H=$H "$VPOINTER" < /tmp/vpointer.fifo >"$OUT/vpointer.log" 2>&1 & PIDS+=($!)
  exec 3>/tmp/vpointer.fifo
  move()  { echo "move $1 $2" >&3; }
  click() { echo click >&3; }
  shot()  { grim "$1"; }
else
  export DISPLAY=${DISPLAY:-:99}; unset WAYLAND_DISPLAY
  Xvfb "$DISPLAY" -screen 0 ${W}x${H}x24 >"$OUT/xvfb.log" 2>&1 & PIDS+=($!)
  for _ in $(seq 50); do xdpyinfo >/dev/null 2>&1 && break; sleep 0.2; done
  # compositing manager so ARGB (transparent) windows blend; visible background to check it
  xsetroot -solid "#3a6ea5"
  xcompmgr >"$OUT/xcompmgr.log" 2>&1 & PIDS+=($!)
  BG_CLIENT=${BG_CLIENT:-xterm -geometry 120x40+500+300 -e $BG_TEXT_CMD}
  move()  { xdotool mousemove "$1" "$2"; }
  click() { xdotool click 1; }
  shot()  { import -window root "$1"; }
fi

# visible client behind the app (set BG_CLIENT=true to disable)
eval "$BG_CLIENT" >/dev/null 2>&1 & PIDS+=($!)
sleep 1
export TH_SECONDS=${TH_SECONDS:-10} TH_DEBUG=${TH_DEBUG:-1}   # app quits by itself, logs per frame
if [ "$APP" != none ]; then
  "$APP" "$@" >"$LOG" 2>&1 & PIDS+=($!)
fi
sleep "$T_START"

move "$X1" "$Y1";  sleep "$T_MOVE"
click;             sleep "$T_CLICK"
shot "$OUT/$MODE-1.png"; sleep "$T_SHOT"
shot "$OUT/$MODE-2.png"
move "$X2" "$Y2"
shot "$OUT/$MODE-3.png"

cleanup; trap - EXIT
echo "--- app output ($LOG) ---"; cat "$LOG" 2>/dev/null
ls -la "$OUT"/$MODE-*.png
