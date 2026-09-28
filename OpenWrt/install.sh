#!/bin/sh
# Install from a reviewed local checkout, without fetching and executing code.
set -eu
[ "$(id -u)" = 0 ] || { echo 'Run as root.' >&2; exit 1; }
command -v uci >/dev/null || { echo 'This installer requires OpenWrt.' >&2; exit 1; }
command -v curl >/dev/null || { echo 'Install curl and ca-bundle with opkg or apk first.' >&2; exit 1; }
DIR=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
[ -f "$DIR/files/ncut-autologin.sh" ] && [ -f "$DIR/files/ncut-autologin.init" ] && [ -f "$DIR/files/ncut-autologin.config" ]
umask 077
[ ! -x /etc/init.d/ncut-autologin ] || /etc/init.d/ncut-autologin stop
cp "$DIR/files/ncut-autologin.sh" /usr/bin/ncut-autologin
cp "$DIR/files/ncut-autologin.init" /etc/init.d/ncut-autologin
chmod 755 /usr/bin/ncut-autologin /etc/init.d/ncut-autologin
if [ ! -f /etc/config/ncut-autologin ]; then cp "$DIR/files/ncut-autologin.config" /etc/config/ncut-autologin; fi
chmod 600 /etc/config/ncut-autologin
mkdir -p /www/luci-static/resources/view /usr/share/luci/menu.d /usr/share/rpcd/acl.d
cp "$DIR/files/luci-view.js" /www/luci-static/resources/view/ncut-autologin.js
cp "$DIR/files/luci-menu.json" /usr/share/luci/menu.d/ncut-autologin.json
cp "$DIR/files/rpcd-acl.json" /usr/share/rpcd/acl.d/ncut-autologin.json
/etc/init.d/ncut-autologin enable
/etc/init.d/ncut-autologin start
[ ! -x /etc/init.d/rpcd ] || /etc/init.d/rpcd restart
echo 'Installed. Configure /etc/config/ncut-autologin, then enable/start the service.'
