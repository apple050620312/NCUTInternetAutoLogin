#!/bin/sh
# curl and BusyBox only. Credentials never appear in curl's process arguments.
set -u
umask 077
PROBE=${NCUT_PROBE:-http://www.gstatic.com/generate_204}
WORK=$(mktemp -d "${TMPDIR:-/tmp}/ncut-login.XXXXXX") || exit 1
trap 'rm -rf "$WORK"' EXIT
trap 'exit 0' INT TERM

log() { printf '%s\n' "$1"; logger -t ncut-autologin "$1" 2>/dev/null || :; }
request() {
    curl --silent --show-error --connect-timeout 5 --max-time 12 \
        --max-redirs 5 --max-filesize 262144 --proto '=http,https' \
        --proto-redir '=http,https' --noproxy '*' \
        --cookie "$WORK/cookies" --cookie-jar "$WORK/cookies" \
        --output "$WORK/page" --write-out '%{http_code}|%{url_effective}' "$@" 2>"$WORK/error"
}
origin() {
    case "$1" in http://*|https://*) ;; *) return 1 ;; esac
    authority=${1#*://}; authority=${authority%%/*}
    case "$authority" in ''|*@*|*\?*|*\#*) return 1 ;; esac
    printf '%s://%s' "${1%%://*}" "$authority"
}
probe() {
    result=$(request --location "$PROBE") || { STATUS=unstable; return; }
    code=${result%%|*}; effective=${result#*|}
    if [ "$code" = 204 ]; then STATUS=online; return; fi
    PORTAL=
    case "$effective" in */fgtauth\?*) PORTAL=$effective ;; esac
    if [ -z "$PORTAL" ]; then
        redirect=$(sed -nE "s/.*(window\.)?location(\.href)?[[:space:]]*=[[:space:]]*['\"]([^'\"]+)['\"].*/\3/p" "$WORK/page" | head -n 1 | sed 's/&amp;/\&/g')
        case "$redirect" in
            http://*/fgtauth\?*|https://*/fgtauth\?*) PORTAL=$redirect ;;
            /fgtauth\?*) PORTAL="$(origin "$effective")$redirect" ;;
        esac
    fi
    if [ -n "$PORTAL" ] && origin "$PORTAL" >/dev/null; then STATUS=needs_login; else STATUS=unstable; fi
}
# Fortinet's small HTML forms use quoted attributes. Ignore unrelated tags.
attribute() {
    sed -nE "s/.*[[:space:]]$1[[:space:]]*=[[:space:]]*\"([^\"]*)\".*/\1/p; t; s/.*[[:space:]]$1[[:space:]]*=[[:space:]]*'([^']*)'.*/\1/p" | head -n 1
}
login() {
    if [ -z "$USERNAME" ] || [ -z "$PASSWORD" ]; then STATUS=missing_credentials; return; fi
    gateway=$(origin "$PORTAL") || { STATUS=login_failed; return; }
    result=$(request --location "$PORTAL") || { STATUS=login_failed; return; }
    effective=${result#*|}
    [ "$(origin "$effective")" = "$gateway" ] || { STATUS=login_failed; return; }
    tr '\n' ' ' < "$WORK/page" | grep -qiE '<title[^>]*>[^<]*勤益科技大學' || { STATUS=login_failed; return; }
    sed 's/>/>\n/g' "$WORK/page" > "$WORK/tags"
    grep -i '<input' "$WORK/tags" | grep -qE "name[[:space:]]*=[[:space:]]*['\"]username['\"]" || { STATUS=login_failed; return; }
    grep -i '<input' "$WORK/tags" | grep -qE "name[[:space:]]*=[[:space:]]*['\"]password['\"]" || { STATUS=login_failed; return; }
    token=$(grep -i '<input' "$WORK/tags" | grep -E "name[[:space:]]*=[[:space:]]*['\"]magic['\"]" | attribute value)
    if [ -z "$token" ]; then token=${PORTAL#*\?}; token=${token%%&*}; fi
    [ -n "$token" ] || { STATUS=login_failed; return; }
    action=$(grep -i '<form' "$WORK/tags" | head -n 1 | attribute action)
    case "$action" in
        '') target="$gateway/" ;;
        http://*|https://*) target=$action ;;
        //*) STATUS=login_failed; return ;;
        /*) target="$gateway$action" ;;
        *) target="$gateway/$action" ;;
    esac
    [ "$(origin "$target")" = "$gateway" ] || { STATUS=login_failed; return; }
    printf '%s' "$USERNAME" > "$WORK/username"
    printf '%s' "$PASSWORD" > "$WORK/password"
    printf '%s' "$token" | sed 's/&amp;/\&/g; s/&quot;/"/g; s/&#39;/'"'"'/g' > "$WORK/magic"
    # Do not follow POST redirects: a 307 could disclose credentials.
    result=$(request --header "Origin: $gateway" --referer "$PORTAL" \
        --data-urlencode "4Tredir=$PROBE" --data-urlencode "magic@$WORK/magic" \
        --data-urlencode "username@$WORK/username" --data-urlencode "password@$WORK/password" "$target") || { STATUS=login_failed; return; }
    rm -f "$WORK/password" "$WORK/username" "$WORK/magic"
    case "${result%%|*}" in 2??) ;; *) STATUS=login_failed; return ;; esac
    grep -qi '/keepalive?' "$WORK/page" || { STATUS=login_failed; return; }
    probe
    [ "$STATUS" = online ] || STATUS=login_failed
}
load_settings() {
    USERNAME=$(uci -q get ncut-autologin.main.username) || USERNAME=
    PASSWORD=$(uci -q get ncut-autologin.main.password) || PASSWORD=
    INTERVAL=$(uci -q get ncut-autologin.main.interval) || INTERVAL=15
    case "$INTERVAL" in ''|*[!0-9]*) INTERVAL=15 ;; esac
    if [ "$INTERVAL" -lt 5 ] || [ "$INTERVAL" -gt 3600 ]; then INTERVAL=15; fi
}
case "${1:-run}" in
    check) probe; printf '%s\n' "$STATUS"; [ "$STATUS" = online ]; exit $? ;;
    login) load_settings; probe; [ "$STATUS" != needs_login ] || login; printf '%s\n' "$STATUS"; [ "$STATUS" = online ]; exit $? ;;
    run) ;;
    *) printf 'Usage: %s [check|login|run]\n' "$0" >&2; exit 1 ;;
esac
previous=
delay=15
while :; do
    load_settings
    probe
    [ "$STATUS" != needs_login ] || login
    if [ "$STATUS" != "$previous" ]; then log "$STATUS"; previous=$STATUS; fi
    case "$STATUS" in online) delay=$INTERVAL ;; *) delay=$((delay * 2)); [ "$delay" -ge "$INTERVAL" ] || delay=$INTERVAL; [ "$delay" -le 300 ] || delay=300 ;; esac
    sleep "$delay" & wait $!
done
