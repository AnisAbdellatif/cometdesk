#!/bin/sh
# Launch the debug app against the dev Comet and screenshot its window to /tmp/shots/<name>.png.
# usage: shot.sh <name> [COMETDESK_DEBUG_UI value]   (run from the repo root)
set -e
name=${1:?name}; view=${2:-}
mkdir -p /tmp/shots
[ -x /tmp/wid ] || swiftc -O .impeccable/tools/wid.swift -o /tmp/wid
cargo build -p cometdesk-gui 2>&1 | tail -1
COMETDESK_DEBUG_UI=$view COMETDESK_AUTOCONNECT=${COMETDESK_AUTOCONNECT:-1} ./target/debug/cometdesk-gui >/tmp/shots/$name.log 2>&1 &
pid=$!
sleep "${SHOT_WAIT:-14}"
id=$(/tmp/wid | awk '/^[0-9]+ \{/{id=$1} /Width = 1280/{print id}' | tail -1)
[ -n "$id" ] && screencapture -x -l "$id" /tmp/shots/$name.png && echo /tmp/shots/$name.png
kill $pid 2>/dev/null || true
