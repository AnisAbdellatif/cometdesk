#!/bin/sh
# Removes glkvm-bridge and its USB network function from a Comet. Run on the device.
# Restarting kvmd-otg re-enumerates USB: the target's keyboard and mouse drop for a moment.
set -u
[ -x /etc/init.d/S99glkvm-bridge ] && /etc/init.d/S99glkvm-bridge stop >/dev/null
rm -rf /etc/init.d/S99glkvm-bridge /usr/local/lib/glkvm-bridge /usr/share/kvmd/extras/glkvm-bridge \
	/etc/glkvm-bridge
kill -HUP "$(cat /run/kvmd/nginx.pid)"
if [ -f /etc/kvmd/override.d/90-glkvm-rs-rndis.yaml ]; then
	rm /etc/kvmd/override.d/90-glkvm-rs-rndis.yaml
	/etc/init.d/S50kvmd-otg restart >/dev/null
fi
echo GLKVM_REMOVE_OK
