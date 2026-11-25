#!/bin/sh
# curl and BusyBox only. Credentials never appear in curl's process arguments.
set -u
umask 077
export LC_ALL=C
PROBE=${NCUT_PROBE:-http://www.gstatic.com/generate_204}
WORK=$(mktemp -d "${TMPDIR:-/tmp}/ncut-login.XXXXXX") || exit 1
CR=$(printf '\r')
SLEEP_PID=
trap 'if [ -n "$SLEEP_PID" ]; then kill "$SLEEP_PID" 2>/dev/null || :; fi; rm -rf "$WORK"' EXIT
trap 'exit 0' INT TERM

log() {
    printf '%s\n' "$1"
    logger -t ncut-autologin "$1" 2>/dev/null || :
}

request() {
    response=$(curl --disable --silent --show-error --globoff \
        --connect-timeout 4 --max-time 12 --max-redirs 5 --max-filesize 262144 \
        --proto '=http,https' --proto-redir '=http,https' --noproxy '*' \
        --header 'Cache-Control: no-cache' \
        --cookie "$WORK/cookies" --cookie-jar "$WORK/cookies" \
        --output "$WORK/page" --write-out '%{http_code}\n%{url_effective}\n%{redirect_url}\n' \
        "$@" 2>"$WORK/error") || return 1
    {
        IFS= read -r HTTP_CODE
        IFS= read -r EFFECTIVE
        IFS= read -r LOCATION || :
    } <<EOF
$response
EOF
    # Native Windows curl writes CRLF in the local protocol fixtures.
    HTTP_CODE=${HTTP_CODE%"$CR"}
    EFFECTIVE=${EFFECTIVE%"$CR"}
    LOCATION=${LOCATION%"$CR"}
}

origin() {
    case "$1" in http://*|https://*) ;; *) return 1 ;; esac
    case "$1" in *\\*|*[[:space:][:cntrl:]]*) return 1 ;; esac
    authority=${1#*://}
    authority=${authority%%[/?#]*}
    case "$authority" in ''|*@*|*%*) return 1 ;; esac
    case "$1" in http://*) authority=${authority%:80} ;; https://*) authority=${authority%:443} ;; esac
    # URL authorities use ASCII case folding. BusyBox 1.30 misaligns tr's case classes.
    # shellcheck disable=SC2018,SC2019
    printf '%s://%s\n' "${1%%://*}" "$authority" | tr 'A-Z' 'a-z'
}

url_path() {
    address=${1#*://}
    case "$address" in
        */*) printf '/%s\n' "${address#*/}" ;;
        *) printf '/\n' ;;
    esac
}

resolve_url() {
    case "$1" in
        http://*|https://*) printf '%s\n' "$1" ;;
        ''|//*|*://*) return 1 ;;
        /*)
            base_origin=$(origin "$2") || return 1
            printf '%s%s\n' "$base_origin" "$1"
            ;;
        *) base=${2%%\?*}; printf '%s/%s\n' "${base%/*}" "$1" ;;
    esac
}

decode_html() {
    sed 's/&quot;/"/g; s/&#39;/'"'"'/g; s/&amp;/\&/g'
}

script_redirect() {
    sed -nE "s/.*location[[:space:]]*(\\.href[[:space:]]*=|=|\\.(assign|replace)[[:space:]]*\\()[[:space:]]*['\"]([^'\"]+)['\"].*/\\3/p" \
        "$WORK/page" | head -n 1 | decode_html
}

is_challenge() {
    case "$(url_path "$1")" in /fgtauth\??*|/login\??*) return 0 ;; esac
    return 1
}

probe() {
    STATUS=unstable
    CHALLENGE=
    request --location --max-time 4 "$PROBE" || return 0
    if [ "$HTTP_CODE" = 204 ] && [ "$EFFECTIVE" = "$PROBE" ]; then
        STATUS=online
        return 0
    fi
    [ "$VALID_GATEWAY" = 1 ] || return 0
    discovered=$EFFECTIVE
    if ! is_challenge "$discovered"; then
        discovered=$(resolve_url "$(script_redirect)" "$EFFECTIVE") || return 0
    fi
    is_challenge "$discovered" || return 0
    discovered_origin=$(origin "$discovered") || return 0
    if [ -z "$GATEWAY" ] || [ "$discovered_origin" = "$GATEWAY" ]; then
        CHALLENGE=$discovered
        STATUS=needs_login
    fi
}

attribute() {
    sed -nE "s/.*[[:space:]]$1[[:space:]]*=[[:space:]]*\"([^\"]*)\".*/\\1/p; t; s/.*[[:space:]]$1[[:space:]]*=[[:space:]]*'([^']*)'.*/\\1/p" \
        | head -n 1 | decode_html
}

parse_form() {
    [ "$(origin "$EFFECTIVE")" = "$1" ] || return 1
    case "$HTTP_CODE" in 2??) ;; *) return 1 ;; esac
    # The portal form uses quoted attributes. Split the HTML into tags so wrapped
    # attributes work across BusyBox and other awk implementations.
    awk -v RS='<' -v apostrophe="'" '
        {
            gsub(/[\r\n]/, " ")
            tag = tolower($0)
            if (tag ~ /^title[[:space:]>]/ &&
                index(substr($0, index($0, ">") + 1), "勤益科技大學")) ncut = 1
            if (tag ~ /^form[[:space:]>]/) {
                in_form = 1
                form = ""
                account = secret = 0
            }
            if (!in_form) next
            form = form "<" $0 "\n"
            if (tag ~ /^input[[:space:]]/) {
                gsub(apostrophe, "\"", tag)
                if (tag ~ /[[:space:]]name[[:space:]]*=[[:space:]]*"username"/) account = 1
                if (tag ~ /[[:space:]]name[[:space:]]*=[[:space:]]*"password"/) secret = 1
            }
            if (tag ~ /^\/form[[:space:]>]/) {
                if (account && secret && !login) login = form
                in_form = 0
            }
        }
        END { if (ncut && login) printf "%s", login }
    ' "$WORK/page" > "$WORK/form"
    [ -s "$WORK/form" ] || return 1
    FORM_URL=$EFFECTIVE
    MAGIC=$(grep -iE "^<input.*[[:space:]]name[[:space:]]*=[[:space:]]*['\"]magic['\"]" "$WORK/form" | attribute value)
    if [ -z "$MAGIC" ]; then
        case "$(url_path "$FORM_URL")" in
            /fgtauth\?*)
                MAGIC=${FORM_URL#*\?}
                MAGIC=${MAGIC%%&*}
                case "$MAGIC" in *=*) return 1 ;; esac
                ;;
        esac
    fi
    [ -n "$MAGIC" ] || return 1
    RETURN_URL=$(grep -iE "^<input.*[[:space:]]name[[:space:]]*=[[:space:]]*['\"]4Tredir['\"]" "$WORK/form" | attribute value)
    action=$(head -n 1 "$WORK/form" | attribute action)
    TARGET=$(resolve_url "${action:-/}" "$FORM_URL") || return 1
    case "$(url_path "$TARGET")" in /logout|/logout\?*) return 1 ;; esac
    [ "$(origin "$TARGET")" = "$1" ]
}

discovered_form() {
    expected_origin=$(origin "$CHALLENGE") || return 1
    if [ "$CHALLENGE" != "$EFFECTIVE" ]; then
        request --location --max-time 4 "$CHALLENGE" || return 1
    fi
    parse_form "$expected_origin"
}

direct_form() {
    [ -n "$GATEWAY" ] || return 1
    marker=$(hexdump -n 8 -e '8/1 "%02x"' /dev/urandom) || return 1
    [ "${#marker}" -eq 16 ] || return 1
    request --max-time 4 "$GATEWAY/login?$marker" && parse_form "$GATEWAY"
}

submit_form() {
    RENEWED=0
    form_origin=$(origin "$FORM_URL") || return 0
    printf '%s' "$USERNAME" > "$WORK/username"
    printf '%s' "$PASSWORD" > "$WORK/password"
    printf '%s' "$MAGIC" > "$WORK/magic"
    printf '%s' "${RETURN_URL:-$PROBE}" > "$WORK/return-url"
    keepalive=
    # Make any follow-up request a separate GET, so a 307 cannot forward the form body.
    if request --header "Origin: $form_origin" --referer "$FORM_URL" \
        --data-urlencode "4Tredir@$WORK/return-url" --data-urlencode "magic@$WORK/magic" \
        --data-urlencode "username@$WORK/username" --data-urlencode "password@$WORK/password" "$TARGET"; then
        case "$HTTP_CODE" in
            2??) keepalive=$(resolve_url "$(script_redirect)" "$EFFECTIVE") || keepalive= ;;
            3??) keepalive=$LOCATION ;;
        esac
    fi
    rm -f "$WORK/password" "$WORK/username" "$WORK/magic" "$WORK/return-url"
    case "$(url_path "$keepalive")" in /keepalive\??*) ;; *) return 0 ;; esac
    [ "$(origin "$keepalive")" = "$form_origin" ] || return 0
    request --max-time 4 --referer "$TARGET" "$keepalive" || return 0
    case "$HTTP_CODE" in
        2??) if grep -qi '/logout?' "$WORK/page"; then RENEWED=1; fi ;;
    esac
}

verify_online() {
    for retry_delay in 0 1 1; do
        if [ "$retry_delay" -ne 0 ]; then sleep "$retry_delay"; fi
        if request --max-time 4 "$PROBE" && [ "$HTTP_CODE" = 204 ] && [ "$EFFECTIVE" = "$PROBE" ]; then
            return 0
        fi
    done
    return 1
}

recover() {
    [ "$STATUS" != online ] || return 0
    if [ -z "$USERNAME" ] || [ -z "$PASSWORD" ]; then
        if [ "$STATUS" = needs_login ] || [ -n "$GATEWAY" ]; then STATUS=missing_credentials; fi
        return 0
    fi
    if [ "$STATUS" = needs_login ] && discovered_form; then
        :
    elif ! direct_form; then
        if [ "$STATUS" = needs_login ]; then STATUS=login_failed; fi
        return 0
    fi
    submit_form
    # The gateway can complete login even when it closes the POST without a reply.
    if verify_online; then STATUS=online; else STATUS=login_failed; fi
}

read_number() {
    case "$1" in ''|*[!0-9]*) printf '%s\n' "$4"; return ;; esac
    number=$1
    case "$number" in
        0*) number=$(printf '%s\n' "$number" | sed 's/^0*//'); number=${number:-0} ;;
    esac
    if [ "${#number}" -le 4 ] && [ "$number" -ge "$2" ] && [ "$number" -le "$3" ]; then
        printf '%s\n' "$number"
    else
        printf '%s\n' "$4"
    fi
}

load_gateway() {
    GATEWAY=
    VALID_GATEWAY=1
    configured=$(uci -q get ncut-autologin.main.portal_url) || configured=
    [ -n "$configured" ] || return 0
    case "$configured" in
        *\?*|*\#*) VALID_GATEWAY=0; return 0 ;;
    esac
    case "$(url_path "$configured")" in
        /|/login) GATEWAY=$(origin "$configured") || VALID_GATEWAY=0 ;;
        *) VALID_GATEWAY=0 ;;
    esac
}

load_settings() {
    load_gateway
    USERNAME=$(uci -q get ncut-autologin.main.username) || USERNAME=
    PASSWORD=$(uci -q get ncut-autologin.main.password) || PASSWORD=
    interval=$(uci -q get ncut-autologin.main.interval) || interval=
    renewal=$(uci -q get ncut-autologin.main.renewal_minutes) || renewal=
    INTERVAL=$(read_number "$interval" 5 3600 15)
    RENEWAL_SECONDS=0
    if [ -n "$GATEWAY" ]; then
        RENEWAL_SECONDS=$(($(read_number "$renewal" 0 1440 30) * 60))
    fi
}

uptime_seconds() { cut -d. -f1 /proc/uptime; }

case "${1:-run}" in
    check) load_gateway; probe; printf '%s\n' "$STATUS"; [ "$STATUS" = online ]; exit $? ;;
    login) load_settings; probe; recover; printf '%s\n' "$STATUS"; [ "$STATUS" = online ]; exit $? ;;
    run) ;;
    *) printf 'Usage: %s [check|login|run]\n' "$0" >&2; exit 1 ;;
esac

previous=
policy=
deadline=0
delay=15
while :; do
    load_settings
    if [ "$policy" != "$GATEWAY|$RENEWAL_SECONDS" ]; then
        policy="$GATEWAY|$RENEWAL_SECONDS"
        deadline=$(($(uptime_seconds) + RENEWAL_SECONDS))
    fi
    probe
    if [ "$STATUS" != online ]; then
        recover
        if [ "$STATUS" = online ]; then deadline=$(($(uptime_seconds) + RENEWAL_SECONDS)); fi
    elif [ "$RENEWAL_SECONDS" -gt 0 ] && [ "$(uptime_seconds)" -ge "$deadline" ]; then
        RENEWED=0
        if [ -n "$USERNAME" ] && [ -n "$PASSWORD" ] && direct_form; then
            submit_form
            if ! verify_online; then STATUS=login_failed; fi
        fi
        if [ "$RENEWED" = 1 ] && [ "$STATUS" = online ]; then
            log renewed
        elif [ "$STATUS" = online ]; then
            log renewal_unconfirmed
        fi
        deadline=$(($(uptime_seconds) + RENEWAL_SECONDS))
    fi
    if [ "$STATUS" != "$previous" ]; then log "$STATUS"; previous=$STATUS; fi
    if [ "$STATUS" = online ]; then
        delay=$INTERVAL
        if [ "$RENEWAL_SECONDS" -gt 0 ]; then
            remaining=$((deadline - $(uptime_seconds)))
            if [ "$remaining" -lt 1 ]; then remaining=1; fi
            if [ "$remaining" -lt "$delay" ]; then delay=$remaining; fi
        fi
    else
        delay=$((delay * 2))
        if [ "$delay" -lt "$INTERVAL" ]; then delay=$INTERVAL; fi
        if [ "$delay" -gt 300 ]; then delay=300; fi
    fi
    sleep "$delay" &
    SLEEP_PID=$!
    wait "$SLEEP_PID"
    SLEEP_PID=
done
