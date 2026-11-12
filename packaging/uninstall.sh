#!/bin/sh
# Turns the firmware limit off (normal charging to 100%) and removes chargelimit.
# Run with: sudo sh uninstall.sh
set -e
[ "$(id -u)" = 0 ] || { echo "run with sudo"; exit 1; }
PLIST=/Library/LaunchDaemons/local.chargelimit.plist
launchctl bootout system "$PLIST" 2>/dev/null || true
/usr/local/bin/chargelimit off 2>/dev/null || true
rm -f "$PLIST" /usr/local/bin/chargelimit /usr/local/etc/chargelimit.conf /var/log/chargelimit.log
pkgutil --forget local.chargelimit >/dev/null 2>&1 || true
echo "chargelimit removed; charging limit is off."
