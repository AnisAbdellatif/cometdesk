#!/bin/sh
# Installs, updates or repairs glkvm-bridge on a Comet. Idempotent: running it again only
# replaces what differs, so it is also the repair after a firmware upgrade.
#
# Run on the device from a directory holding the bridge files (glkvm-gui and `glkvm bridge
# setup` stage them in /tmp/glkvm-setup through the web terminal). Prints one line per
# change, and GLKVM_SETUP_OK <version> on success.
#
# Restarting kvmd-otg (only when the RNDIS function is missing or its config changed)
# re-enumerates USB: the target's keyboard and mouse drop for a moment.
set -eu
SRC=$(cd "$(dirname "$0")" && pwd)
BASE=/usr/local/lib/glkvm-bridge
EXTRA=/usr/share/kvmd/extras/glkvm-bridge
OVERRIDE=/etc/kvmd/override.d/90-glkvm-rs-rndis.yaml

mkdir -p "$BASE/target" "$BASE/agents" "$EXTRA" /etc/glkvm-bridge /etc/kvmd/override.d

# put <src> <dest> <mode>: replace dest only if it differs
put() {
	if ! cmp -s "$1" "$2"; then
		cp "$1" "$2.new"
		chmod "$3" "$2.new"
		mv -f "$2.new" "$2"
		echo "updated $2"
	fi
}

put "$SRC/glkvm_bridge.py" "$BASE/glkvm_bridge.py" 755
put "$SRC/install.sh" "$BASE/target/install.sh" 644
put "$SRC/install.ps1" "$BASE/target/install.ps1" 644
for f in manifest.yaml nginx.ctx-http.conf nginx.ctx-server.conf; do
	put "$SRC/$f" "$EXTRA/$f" 644
done
put "$SRC/udhcpd.conf" /etc/glkvm-bridge/udhcpd.conf 644
put "$SRC/S99glkvm-bridge" /etc/init.d/S99glkvm-bridge 755
# Identifies the app build that installed these files (glkvm::api::bridge::bundle_id).
[ -f "$SRC/bundle" ] && put "$SRC/bundle" "$BASE/bundle" 644

restart_otg=0
if ! cmp -s "$SRC/90-glkvm-rs-rndis.yaml" "$OVERRIDE"; then
	put "$SRC/90-glkvm-rs-rndis.yaml" "$OVERRIDE" 644
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

/etc/init.d/S99glkvm-bridge restart >/dev/null
nginx -t -q -p /etc/kvmd/nginx -c /etc/kvmd/nginx-kvmd.conf
kill -HUP "$(cat /run/kvmd/nginx.pid)"

for _ in 1 2 3 4 5 6 7 8 9 10; do
	[ -S /run/kvmd/glkvm-bridge.sock ] && break
	sleep 1
done
version=$(sed -n 's/^VERSION = "\(.*\)"$/\1/p' "$BASE/glkvm_bridge.py")
echo "GLKVM_SETUP_OK $version"
