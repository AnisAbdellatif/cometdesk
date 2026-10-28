#!/bin/sh
# Remove the glkvm-bridge prototype and the RNDIS gadget override from a Comet.
# Usage: bridge/uninstall.sh [root@<comet>]   (default root@192.168.8.174)
# Restarting kvmd-otg re-enumerates USB: the target's keyboard/mouse drop briefly.
set -eu
HOST=${1:-root@192.168.8.174}

ssh "$HOST" 'set -e
[ -x /etc/init.d/S99glkvm-bridge ] && /etc/init.d/S99glkvm-bridge stop
rm -rf /etc/init.d/S99glkvm-bridge /usr/local/lib/glkvm-bridge /usr/share/kvmd/extras/glkvm-bridge /etc/glkvm-bridge
kill -HUP "$(cat /run/kvmd/nginx.pid)"
if [ -f /etc/kvmd/override.d/90-glkvm-rs-rndis.yaml ]; then
	rm /etc/kvmd/override.d/90-glkvm-rs-rndis.yaml
	/etc/init.d/S50kvmd-otg restart
fi
echo "glkvm-bridge removed"'
