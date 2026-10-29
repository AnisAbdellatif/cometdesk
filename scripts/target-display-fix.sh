#!/usr/bin/env bash
# Prototype: make the Comet (HDMI) the display at GRUB and at the GDM login screen on an Ubuntu target.
# Run on the target as root: sudo bash target-display-fix.sh [user]
set -euo pipefail
user=${1:-${SUDO_USER:?run with sudo}}
home=$(getent passwd "$user" | cut -d: -f6)

# GDM 50 runs the greeter as gdm-greeter with HOME=/run/gdm3/home/gdm-greeter (tmpfs), so
# /var/lib/gdm3/.config is ignored; mutter also reads the system-wide /etc/xdg/monitors.xml.
install -o root -g root -m 0644 "$home/.config/monitors.xml" /etc/xdg/monitors.xml

# GDM login screen (older GDM): reuse the user's monitor layout (Comet primary, laptop panel disabled).
install -d -o gdm -g gdm /var/lib/gdm3/.config
install -o gdm -g gdm -m 0644 "$home/.config/monitors.xml" /var/lib/gdm3/.config/monitors.xml

# Keep the laptop awake with the lid closed (firmware/GDM then drive only the external display).
install -d /etc/systemd/logind.conf.d
cat > /etc/systemd/logind.conf.d/10-comet.conf <<CONF
[Login]
HandleLidSwitchDocked=ignore
HandleLidSwitchExternalPower=ignore
CONF

# GRUB: visible menu, fixed mode, text console.
install -d /etc/default/grub.d
cat > /etc/default/grub.d/90-comet.cfg <<CONF
GRUB_TIMEOUT_STYLE=menu
GRUB_TIMEOUT=5
GRUB_GFXMODE=1920x1080,auto
GRUB_GFXPAYLOAD_LINUX=keep
GRUB_TERMINAL=console
CONF
update-grub
grep -nE "^set (timeout|timeout_style|gfxmode)|terminal_output|gfxpayload" /boot/grub/grub.cfg | head -20 > /tmp/grub-check.txt; chmod 644 /tmp/grub-check.txt
echo "done; reboot to apply (logind lid settings need a reboot or 'systemctl restart systemd-logind')"
