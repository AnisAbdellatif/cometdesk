#!/bin/sh
# Install or update the glkvm-bridge prototype on a Comet over SSH.
# Usage: bridge/install.sh [root@<comet>]   (default root@192.168.8.174)
# Restarting kvmd-otg re-enumerates USB: the target's keyboard/mouse drop briefly.
set -eu
HOST=${1:-root@192.168.8.174}
DIR=$(cd "$(dirname "$0")" && pwd)

ssh "$HOST" 'mkdir -p /usr/local/lib/glkvm-bridge /usr/share/kvmd/extras/glkvm-bridge /etc/glkvm-bridge /etc/kvmd/override.d'
scp -q "$DIR/glkvm_bridge.py" "$HOST:/usr/local/lib/glkvm-bridge/glkvm_bridge.py"
scp -q "$DIR"/extra/* "$HOST:/usr/share/kvmd/extras/glkvm-bridge/"
scp -q "$DIR/udhcpd.conf" "$HOST:/etc/glkvm-bridge/udhcpd.conf"
scp -q "$DIR/S99glkvm-bridge" "$HOST:/etc/init.d/S99glkvm-bridge"

ssh "$HOST" 'set -e
chmod 755 /etc/init.d/S99glkvm-bridge /usr/local/lib/glkvm-bridge/glkvm_bridge.py
f=/etc/kvmd/override.d/90-glkvm-rs-rndis.yaml
if [ ! -f $f ]; then
	cat > $f <<EOF
# glkvm-rs: USB network (RNDIS) for the clipboard bridge. Remove with bridge/uninstall.sh.
otg:
    devices:
        ethernet:
            enabled: true
            driver: rndis
            start: true
            host_mac: "02:47:4b:56:4d:01"
            kvm_mac: "02:47:4b:56:4d:02"
EOF
	/etc/init.d/S50kvmd-otg restart
fi
/etc/init.d/S99glkvm-bridge restart
nginx -t -p /etc/kvmd/nginx -c /etc/kvmd/nginx-kvmd.conf 2>&1 | tail -1
kill -HUP "$(cat /run/kvmd/nginx.pid)"
echo "glkvm-bridge installed"'
