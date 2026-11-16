#!/bin/sh
# Launch the debug app against the dev Comet and screenshot its window to $TMPDIR/shots/<name>.png.
# $TMPDIR is per-user on macOS; a shared /tmp would let another account plant `wid` for us to run.
# usage: shot.sh <name> [COMETDESK_DEBUG_UI value]   (run from the repo root)
set -e
name=${1:?name}; view=${2:-}
t=${TMPDIR:?TMPDIR unset}; shots=$t/shots; wid=$t/cometdesk-wid
mkdir -p "$shots"
[ -x "$wid" ] || swiftc -O .impeccable/tools/wid.swift -o "$wid"
cargo build -p cometdesk-gui 2>&1 | tail -1
COMETDESK_DEBUG_UI=$view COMETDESK_AUTOCONNECT=${COMETDESK_AUTOCONNECT:-1} ./target/debug/cometdesk-gui >"$shots/$name.log" 2>&1 &
pid=$!
sleep "${SHOT_WAIT:-14}"
id=$("$wid" | awk '/^[0-9]+ \{/{id=$1} /Width = 1280/{print id}' | tail -1)
[ -n "$id" ] && screencapture -x -l "$id" "$shots/$name.png" && echo "$shots/$name.png"
kill $pid 2>/dev/null || true
