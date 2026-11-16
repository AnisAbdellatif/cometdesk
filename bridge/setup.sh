#!/bin/sh
# Installs, updates or repairs cometdesk-bridge on a Comet. Idempotent: running it again only
# replaces what differs, so it is also the repair after a firmware upgrade.
#
# Run on the device from a directory holding the bridge files (cometdesk-gui and `cometdesk bridge
# setup` stage them in a fresh `mktemp -d` folder through the web terminal). Prints one line per
# change, and COMETDESK_SETUP_OK <version> on success.
#
# Restarting kvmd-otg (only when the RNDIS function is missing or its config changed)
# re-enumerates USB: the target's keyboard and mouse drop for a moment.
set -eu
SRC=$(cd "$(dirname "$0")" && pwd)
BASE=/usr/local/lib/cometdesk-bridge
EXTRA=/usr/share/kvmd/extras/cometdesk-bridge
OVERRIDE=/etc/kvmd/override.d/90-cometdesk-rndis.yaml

# Remove what the app's former name (glkvm) installed: two bridges would fight for the port.
if [ -e /etc/init.d/S99glkvm-bridge ]; then
	/etc/init.d/S99glkvm-bridge stop >/dev/null 2>&1
	echo "removed glkvm-bridge"
fi
rm -rf /etc/init.d/S99glkvm-bridge /usr/local/lib/glkvm-bridge /usr/share/kvmd/extras/glkvm-bridge \
	/etc/glkvm-bridge /run/kvmd/glkvm-bridge.sock
OLD=/etc/kvmd/override.d/90-glkvm-rs-rndis.yaml
OLD2=/etc/kvmd/override.d/90-cometdesk-rs-rndis.yaml
if [ -f $OLD ] || [ -f $OLD2 ]; then
	rm -f $OLD $OLD2
	echo "removed old USB network override"
fi

mkdir -p "$BASE/target" "$BASE/agents" "$EXTRA" /etc/cometdesk-bridge /etc/kvmd/override.d
# The daemon drops to an unprivileged user: it must be able to traverse every parent of
# $BASE to read the installers and agents (a 0700 /usr/local/lib gave the target 404s).
d=$BASE
while [ "$d" != / ]; do
	chmod o+x "$d" 2>/dev/null || true
	d=$(dirname "$d")
done
# The daemon hands $BASE/agents (only that folder, not what is in it) to its user itself.

# reload_nginx: HUP the nginx master named by kvmd's pidfile. /run/kvmd is writable by the
# kvmd account, so only trust a regular file holding a number that names an nginx process.
reload_nginx() {
	f=/run/kvmd/nginx.pid
	[ -f "$f" ] && [ ! -L "$f" ] || return 0
	pid=$(head -c 16 "$f")
	case "$pid" in '' | *[!0-9]*) return 0 ;; esac
	[ "$(cat "/proc/$pid/comm" 2>/dev/null)" = nginx ] && kill -HUP "$pid"
	return 0
}

# put <src> <dest> <mode>: replace dest only if it differs
put() {
	if ! cmp -s "$1" "$2"; then
		cp "$1" "$2.new"
		chmod "$3" "$2.new"
		mv -f "$2.new" "$2"
		echo "updated $2"
	fi
}

put "$SRC/cometdesk_bridge.py" "$BASE/cometdesk_bridge.py" 755
if [ -f "$SRC/target/install.sh" ]; then
	put "$SRC/target/install.sh" "$BASE/target/install.sh" 644
else
	put "$SRC/install.sh" "$BASE/target/install.sh" 644
fi
if [ -f "$SRC/target/install.ps1" ]; then
	put "$SRC/target/install.ps1" "$BASE/target/install.ps1" 644
else
	put "$SRC/install.ps1" "$BASE/target/install.ps1" 644
fi
for f in manifest.yaml nginx.ctx-http.conf nginx.ctx-server.conf; do
	put "$SRC/$f" "$EXTRA/$f" 644
done
put "$SRC/udhcpd.conf" /etc/cometdesk-bridge/udhcpd.conf 644
put "$SRC/S99cometdesk-bridge" /etc/init.d/S99cometdesk-bridge 755
# Identifies the app build that installed these files (cometdesk::api::bridge::bundle_id).
[ -f "$SRC/bundle" ] && put "$SRC/bundle" "$BASE/bundle" 644

restart_otg=0
if ! cmp -s "$SRC/90-cometdesk-rndis.yaml" "$OVERRIDE"; then
	put "$SRC/90-cometdesk-rndis.yaml" "$OVERRIDE" 644
	restart_otg=1
fi
if ! ls /sys/kernel/config/usb_gadget/*/configs/*/rndis.usb0 >/dev/null 2>&1; then
	echo "USB network function missing"
	restart_otg=1
fi
if [ "$restart_otg" = 1 ]; then
	echo "restarting kvmd-otg (USB re-enumerates)"
	/etc/init.d/S50kvmd-otg restart >/dev/null
	for _ in 1 2 3 4 5 6 7 8 9 10; do
		[ -e /sys/class/net/usb0 ] && break
		sleep 1
	done
fi

/etc/init.d/S99cometdesk-bridge restart >/dev/null
nginx -t -q -p /etc/kvmd/nginx -c /etc/kvmd/nginx-kvmd.conf
reload_nginx

for _ in 1 2 3 4 5 6 7 8 9 10; do
	[ -S /run/cometdesk-bridge/bridge.sock ] && break
	sleep 1
done
version=$(sed -n 's/^VERSION = "\(.*\)"$/\1/p' "$BASE/cometdesk_bridge.py")
echo "COMETDESK_SETUP_OK $version"
