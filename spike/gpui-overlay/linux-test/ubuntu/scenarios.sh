#!/usr/bin/env bash
# Inside the test container. Usage: scenarios.sh [sway] [x11] [gnome]
APP=/usr/bin/tinyhand-overlay
OUT=/out; W=1920; H=1080
mkdir -p /tmp/.X11-unix; chmod 1777 /tmp/.X11-unix   # a real distro creates this at boot
mkdir -p "$XDG_RUNTIME_DIR" $OUT; chmod 700 "$XDG_RUNTIME_DIR"
export TH_SECONDS=${TH_SECONDS:-12} TH_DEBUG=1
BG_TEXT=(sh -c 'seq 1 200; sleep 60')
PIDS=(); cleanup() { for p in "${PIDS[@]}"; do kill "$p" 2>/dev/null; done; wait 2>/dev/null; PIDS=(); rm -f $XDG_RUNTIME_DIR/wayland-*; }

sway_case() {
  export WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1
  printf 'output HEADLESS-1 resolution %sx%s position 0 0\ndefault_border none\n' $W $H > /tmp/sway.conf
  sway -c /tmp/sway.conf >$OUT/sway-compositor.log 2>&1 & PIDS+=($!)
  for _ in $(seq 50); do ls $XDG_RUNTIME_DIR/wayland-[0-9] >/dev/null 2>&1 && break; sleep 0.2; done
  export WAYLAND_DISPLAY=$(basename $(ls $XDG_RUNTIME_DIR/wayland-[0-9] | head -1)); unset DISPLAY
  rm -f /tmp/vp.fifo; mkfifo /tmp/vp.fifo
  W=$W H=$H /t/vpointer < /tmp/vp.fifo >$OUT/sway-vpointer.log 2>&1 & PIDS+=($!)
  exec 3>/tmp/vp.fifo
  foot "${BG_TEXT[@]}" >/dev/null 2>&1 & PIDS+=($!); sleep 1
  $APP >$OUT/sway-app.log 2>&1 & PIDS+=($!); sleep 3
  echo "move 700 450" >&3; sleep 0.5; echo click >&3; sleep 0.3; grim $OUT/sway-1.png
  sleep 0.4; grim $OUT/sway-2.png; echo "move 1200 600" >&3; sleep 0.3; grim $OUT/sway-3.png
  exec 3>&-; cleanup
}

x11_case() {
  export DISPLAY=:99; unset WAYLAND_DISPLAY
  Xvfb $DISPLAY -screen 0 ${W}x${H}x24 >$OUT/x11-xvfb.log 2>&1 & PIDS+=($!)
  for _ in $(seq 50); do xdpyinfo >/dev/null 2>&1 && break; sleep 0.2; done
  xsetroot -solid "#3a6ea5"; xcompmgr >$OUT/x11-xcompmgr.log 2>&1 & PIDS+=($!)
  xterm -geometry 120x40+500+300 -e "${BG_TEXT[@]}" >/dev/null 2>&1 & PIDS+=($!); sleep 1
  $APP >$OUT/x11-app.log 2>&1 & PIDS+=($!); sleep 3
  xdotool mousemove 700 450; sleep 0.5; xdotool click 1; sleep 0.3; import -window root $OUT/x11-1.png
  sleep 0.4; import -window root $OUT/x11-2.png; xdotool mousemove 1200 600; sleep 0.3; import -window root $OUT/x11-3.png
  cleanup
}

gnome_case() {
  unset DISPLAY WAYLAND_DISPLAY
  cat > /tmp/gnome-session.sh <<INNER
#!/usr/bin/env bash
mutter --headless --wayland --virtual-monitor ${W}x${H} >$OUT/gnome-mutter.log 2>&1 &
MP=\$!
for _ in \$(seq 50); do ls $XDG_RUNTIME_DIR/wayland-[0-9] >/dev/null 2>&1 && break; sleep 0.2; done
export WAYLAND_DISPLAY=\$(basename \$(ls $XDG_RUNTIME_DIR/wayland-[0-9] | head -1))
echo "WAYLAND_DISPLAY=\$WAYLAND_DISPLAY" > $OUT/gnome-env.txt
foot sh -c 'seq 1 200; sleep 60' >$OUT/gnome-foot.log 2>&1 &
sleep 1
# A real GNOME session exports DISPLAY and XAUTHORITY of mutter's XWayland; do the same here.
export XAUTHORITY=\$(ls $XDG_RUNTIME_DIR/.mutter-Xwaylandauth.* | head -1)
export DISPLAY=:\$(ls /tmp/.X11-unix | sed 's/^X//' | sort -n | head -1)
echo "DISPLAY=\$DISPLAY XAUTHORITY=\$XAUTHORITY sockets: \$(ls /tmp/.X11-unix)" >> $OUT/gnome-env.txt
$APP >$OUT/gnome-app.log 2>&1 &
AP=\$!
sleep 4
xdotool mousemove 700 450 >>$OUT/gnome-env.txt 2>&1; sleep 0.5; xdotool click 1 >>$OUT/gnome-env.txt 2>&1; sleep 0.3
xwininfo -root -tree >> $OUT/gnome-env.txt 2>&1
for w in \$(xdotool search --onlyvisible --name . 2>/dev/null); do echo "win \$w \$(xdotool getwindowname \$w) \$(xdotool getwindowgeometry \$w | tr '\\n' ' ')" >> $OUT/gnome-env.txt; timeout 10 import -window \$w $OUT/gnome-win-\$w.png 2>>$OUT/gnome-env.txt; done
timeout 10 import -window 0x600001 $OUT/gnome-appwin.png 2>>$OUT/gnome-env.txt; xwd -root -silent -out /tmp/r.xwd 2>>$OUT/gnome-env.txt && convert /tmp/r.xwd $OUT/gnome-xwd.png 2>>$OUT/gnome-env.txt
timeout 10 import -window root $OUT/gnome-1.png 2>>$OUT/gnome-env.txt
sleep 0.4; timeout 10 import -window root $OUT/gnome-2.png 2>>$OUT/gnome-env.txt
xdotool mousemove 1200 600; sleep 0.3; timeout 10 import -window root $OUT/gnome-3.png 2>>$OUT/gnome-env.txt
wait \$AP
kill \$MP
INNER
  chmod +x /tmp/gnome-session.sh
  dbus-run-session -- /tmp/gnome-session.sh
  cleanup
}

for c in "${@:-sway x11 gnome}"; do
  for s in $c; do echo "=== $s ==="; ${s}_case; echo "--- $s app log (head/tail) ---"; head -5 $OUT/$s-app.log; echo ...; tail -3 $OUT/$s-app.log; echo "frames: $(grep -c '^frame at' $OUT/$s-app.log)"; done
done
