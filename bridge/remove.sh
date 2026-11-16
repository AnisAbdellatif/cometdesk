#!/bin/sh
# Removes cometdesk-bridge and its USB network function from a Comet. Run on the device.
# Restarting kvmd-otg re-enumerates USB: the target's keyboard and mouse drop for a moment.
set -u
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
[ -x /etc/init.d/S99glkvm-bridge ] && /etc/init.d/S99glkvm-bridge stop >/dev/null
[ -x /etc/init.d/S99cometdesk-bridge ] && /etc/init.d/S99cometdesk-bridge stop >/dev/null
rm -rf /etc/init.d/S99cometdesk-bridge /usr/local/lib/cometdesk-bridge /usr/share/kvmd/extras/cometdesk-bridge \
	/etc/cometdesk-bridge /run/cometdesk-bridge /etc/init.d/S99glkvm-bridge /usr/local/lib/glkvm-bridge \
	/usr/share/kvmd/extras/glkvm-bridge /etc/glkvm-bridge
# The extras include is a wildcard glob in kvmd's nginx config, so removing the dir above
# drops our locations with no regeneration needed. Only reload if the config still validates
# and nginx is actually running, so we never HUP a broken config or a stale/empty pidfile.
if nginx -t -q -p /etc/kvmd/nginx -c /etc/kvmd/nginx-kvmd.conf 2>/dev/null; then
	reload_nginx
fi
D=/etc/kvmd/override.d
if [ -f $D/90-cometdesk-rndis.yaml ] || [ -f $D/90-cometdesk-rs-rndis.yaml ] || [ -f $D/90-glkvm-rs-rndis.yaml ]; then
	rm -f $D/90-cometdesk-rndis.yaml $D/90-cometdesk-rs-rndis.yaml $D/90-glkvm-rs-rndis.yaml
	/etc/init.d/S50kvmd-otg restart >/dev/null
fi
echo COMETDESK_REMOVE_OK
