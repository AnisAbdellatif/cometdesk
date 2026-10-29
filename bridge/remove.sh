#!/bin/sh
# Removes glkvm-bridge and its USB network function from a Comet. Run on the device.
# Restarting kvmd-otg re-enumerates USB: the target's keyboard and mouse drop for a moment.
set -u
[ -x /etc/init.d/S99glkvm-bridge ] && /etc/init.d/S99glkvm-bridge stop >/dev/null
rm -rf /etc/init.d/S99glkvm-bridge /usr/local/lib/glkvm-bridge /usr/share/kvmd/extras/glkvm-bridge \
	/etc/glkvm-bridge
# The extras include is a wildcard glob in kvmd's nginx config, so removing the dir above
# drops our locations with no regeneration needed. Only reload if the config still validates
# and nginx is actually running, so we never HUP a broken config or a stale/empty pidfile.
if nginx -t -q -p /etc/kvmd/nginx -c /etc/kvmd/nginx-kvmd.conf 2>/dev/null && [ -s /run/kvmd/nginx.pid ]; then
	kill -HUP "$(cat /run/kvmd/nginx.pid)"
fi
if [ -f /etc/kvmd/override.d/90-glkvm-rs-rndis.yaml ]; then
	rm /etc/kvmd/override.d/90-glkvm-rs-rndis.yaml
	/etc/init.d/S50kvmd-otg restart >/dev/null
fi
echo GLKVM_REMOVE_OK
