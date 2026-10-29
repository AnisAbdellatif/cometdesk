#!/bin/sh
# Installs cometdesk-agent for the current user on a Linux target, pairs it and starts it.
# Served by cometdesk-bridge on the Comet's USB link; cometdesk-gui types the command for you
# (no pipe: `|` needs AltGr on some layouts and may not arrive over HID):
#
#   curl -fsSo /tmp/cometdesk-install.sh http://192.168.5.1:47475/install.sh && sh /tmp/cometdesk-install.sh <code>
#
# No root needed. Installs to ~/.local/bin, autostarts with the desktop session (the
# clipboard belongs to the session), logs to ~/.local/state/cometdesk-agent.log.
set -eu
RELAY=http://192.168.5.1:47475
CODE=${1:-}
if [ -z "$CODE" ]; then
	echo "usage: sh install.sh <pairing-code>" >&2
	exit 2
fi

case "$(uname -s)-$(uname -m)" in
Linux-x86_64 | Linux-amd64) NAME=linux-x86_64 ;;
Linux-aarch64 | Linux-arm64) NAME=linux-aarch64 ;;
*)
	echo "cometdesk-agent: $(uname -sm) is not supported yet" >&2
	exit 1
	;;
esac

fetch() {
	if command -v curl >/dev/null 2>&1; then
		curl -fsS "$1" -o "$2"
	else
		wget -q "$1" -O "$2"
	fi
}

BIN="$HOME/.local/bin"
EXE="$BIN/cometdesk-agent"
LOG="${XDG_STATE_HOME:-$HOME/.local/state}/cometdesk-agent.log"
AUTOSTART="${XDG_CONFIG_HOME:-$HOME/.config}/autostart/cometdesk-agent.desktop"
mkdir -p "$BIN" "$(dirname "$LOG")" "$(dirname "$AUTOSTART")"

echo "Downloading cometdesk-agent ($NAME) from the Comet..."
fetch "$RELAY/agents/$NAME" "$EXE.new"
chmod 755 "$EXE.new"
pkill -x cometdesk-agent 2>/dev/null || true
# Leftovers of the former name (glkvm-agent).
pkill -x glkvm-agent 2>/dev/null || true
rm -f "$BIN/glkvm-agent" "$(dirname "$AUTOSTART")/glkvm-agent.desktop"
mv -f "$EXE.new" "$EXE"

"$EXE" pair "$CODE" >/dev/null

cat >"$AUTOSTART" <<EOF
[Desktop Entry]
Type=Application
Name=CometDesk clipboard agent
Comment=Shares the clipboard with cometdesk-gui through the KVM
Exec=sh -c '"$EXE" run >>"$LOG" 2>&1'
NoDisplay=true
X-GNOME-Autostart-enabled=true
EOF

nohup "$EXE" run >>"$LOG" 2>&1 &
sleep 1
if kill -0 $! 2>/dev/null; then
	echo "cometdesk-agent is installed, paired and running. It starts with your desktop session."
	echo "Log: $LOG    Remove: $EXE uninstall"
else
	echo "cometdesk-agent did not start; see $LOG" >&2
	tail -n 5 "$LOG" >&2
	exit 1
fi
