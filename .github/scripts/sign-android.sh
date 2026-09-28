#!/bin/sh
set -eu
[ -n "${ANDROID_KEYSTORE_BASE64:-}" ] && [ -n "${ANDROID_KEYSTORE_PASSWORD:-}" ] || { echo 'Android signing secrets are missing.' >&2; exit 1; }
umask 077
keyfile="$RUNNER_TEMP/ncut-release.jks"
trap 'rm -f "$keyfile"' EXIT
printf '%s' "$ANDROID_KEYSTORE_BASE64" | base64 --decode > "$keyfile"
node .github/scripts/collect-installer.mjs android
mkdir -p release-assets
"$ANDROID_HOME/build-tools/36.0.0/apksigner" sign --ks "$keyfile" --ks-key-alias ncut \
    --ks-pass env:ANDROID_KEYSTORE_PASSWORD --key-pass env:ANDROID_KEYSTORE_PASSWORD \
    --out release-assets/NCUT-Android.apk .tools/unsigned-android.apk
"$ANDROID_HOME/build-tools/36.0.0/apksigner" verify release-assets/NCUT-Android.apk
rm -f .tools/unsigned-android.apk release-assets/NCUT-Android.apk.idsig
