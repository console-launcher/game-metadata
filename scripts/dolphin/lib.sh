#!/usr/bin/env bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Shared by scripts/dolphin/*: where the private material lives and how Dolphin is driven (own Xvfb display, never the desktop).
# env: WII_REFS  private root, default ~/.local/share/dolphin-wii-refs. Layout under it:
#   nand-dump/  the owner's BootMii backup (read only by Dolphin's own NAND import; these scripts never open the keys file)
#   user/       Dolphin's user dir with the imported NAND
#   discs/      the owner's discs as .rvz
#   out/<slug>/ one capture per title: meta.json and the cropped frames
#   lib/        what `publish` stages (banner archive, reference frames, manifest)
ROOT=${WII_REFS:-$HOME/.local/share/dolphin-wii-refs}
APP=org.DolphinEmu.dolphin-emu
say() { printf '%-8s %s\n' "$1" "$2"; }

# Pixel coordinates in Dolphin's main window (800x533 under Xvfb, Qt's own dialogs): menu entries, measured once on 2609b.
MENU_TOOLS="245 8"; MENU_LOAD_WII_MENU="310 235"; MENU_FILE="20 8"; MENU_CHANGE_DISC="76 53"
MENU_MOVIE="137 8"; MENU_DUMP_FRAMES="185 231"

XPID=""; DPIDS=""
# Own Xvfb display; its pid is recorded and only that pid is killed.
start_xvfb() {
  DISP=":$((200 + RANDOM % 700))"
  Xvfb "$DISP" -screen 0 1280x800x24 >/dev/null 2>&1 &
  XPID=$!
  export DISPLAY="$DISP"
  timeout 20 bash -c 'until xdotool getmouselocation >/dev/null 2>&1; do sleep 0.5; done'
}
# The sandbox processes of this user dir's Dolphin (flatpak hands the launch to a helper, so the pid of `flatpak run` is not
# Dolphin's): what start_dolphin finds new after its launch is this run's, and what stop_all kills.
dolphin_pids() { ps -eo pid=,args= | awk -v u="dolphin-emu-wrapper -u $ROOT/user" 'index($0, "bwrap") == 1 || $2 == "bwrap" { if (index($0, u)) print $1 }'; }
# Dolphin from the flatpak with the private user dir: software GL, no portal dialogs (they would open on the desktop), silent.
start_dolphin() {
  local before=" $(dolphin_pids | tr '\n' ' ') " p
  flatpak run --filesystem="$ROOT" --env=LIBGL_ALWAYS_SOFTWARE=1 --env=QT_QPA_PLATFORMTHEME=generic --env=GTK_USE_PORTAL=0 \
    "$APP" -u "$ROOT/user" >/dev/null 2>&1 &
  timeout 180 bash -c 'until xdotool search --name "Dolphin 2609b" >/dev/null 2>&1; do sleep 2; done'
  for p in $(dolphin_pids); do case "$before" in *" $p "*) ;; *) DPIDS="$DPIDS $p";; esac; done
  sleep 4
}
# The sandbox's init ignores TERM, so KILL the recorded pids; the Xvfb goes after.
stop_all() {
  local p
  for p in $DPIDS; do kill -9 "$p" 2>/dev/null; done
  [ -n "$XPID" ] && kill "$XPID" 2>/dev/null
  true
}
# Wait until the number of visible X windows satisfies `-ge N` / `-le N` (a dialog opening or closing); the machine may be slow.
wait_windows() { timeout "${3:-180}" bash -c "until [ \$(xdotool search --onlyvisible --name . 2>/dev/null | wc -l) $1 $2 ]; do sleep 1; done"; }
click() { xdotool mousemove "$@" click 1; }
# Dolphin polls the pointer: a click shorter than a frame is lost, so hold it.
tap() { xdotool mousemove "$@" mousedown 1; sleep 0.6; xdotool mouseup 1; }
render_window() { xdotool search --name "Wii Menu" 2>/dev/null | head -1; }
