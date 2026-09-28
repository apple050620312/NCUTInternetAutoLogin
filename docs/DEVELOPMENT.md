# Development

## Repository

- `Desktop/`: shared Windows/Linux/macOS Tauri 2 desktop app.
- `OpenWrt/`: POSIX shell + curl, UCI/procd and SDK package.
- `Android/`: Tauri 2 frontend, Kotlin foreground service, Android Keystore.
- `CLI/`: headless Rust executable and systemd unit.
- `Core/`: shared Rust protocol engine, UI status labels and source logo.
- `.github/`: build/release workflows and private signing setup helper.

Implementation priority: Windows, OpenWrt, Linux, Android, macOS. No iOS.

## Local builds

Rust stable and Node.js 22+; platform requirements:
https://v2.tauri.app/start/prerequisites/

```sh
cd Desktop
npm ci
npm run tauri dev
npm run tauri build
```

Windows: MSVC and WebView2. Linux: WebKitGTK 4.1, AppIndicator, D-Bus and an
unlocked Secret Service provider. macOS: Xcode tools and Keychain.
Passwords are saved in the OS keyring (`tw.edu.ncut.autologin`, account name),
not settings JSON. Settings live in Tauri's app configuration directory.

Android requires JDK 17, SDK/NDK and Rust Android targets. The workflow runs
`tauri android init`, applies the manifest configuration with a structured XML
parser, builds the universal APK, then signs it. The foreground service calls
the same Rust core through JNI. Credentials use Keystore-backed AES-GCM and
are excluded from app backup. Automatic monitoring is user-enabled.

```sh
cd Android
npm ci
npm run tauri android init -- --ci
cd ..
node .github/scripts/configure-android.mjs
cd Android
npm run tauri android build -- --apk
```

## Verification

OpenWrt package: copy `OpenWrt/` into an SDK's
`package/ncut-autologin`, select Network / ncut-autologin, then run
`make package/ncut-autologin/compile V=s`. LuCI files are included but do not
force a GUI dependency on headless routers. curl/ca-bundle are dependencies.
The UCI config is installed mode 600; private temporary files keep credentials
out of process arguments. Shell logs status changes and backs off failures.

Headless build: `cargo build -p ncut-cli --release`. Install the binary as
`/usr/local/bin/ncut`, `CLI/ncut.service` in `/etc/systemd/system/`, and a
root-owned mode-600 `/etc/ncut-autologin.env` with `NCUT_USERNAME` and
`NCUT_PASSWORD`. Then `systemctl daemon-reload` and
`systemctl enable --now ncut`. The unit uses an isolated dynamic user.

```sh
cargo test -p ncut-core -p ncut-cli -p ncut-desktop --locked
cargo clippy -p ncut-core -p ncut-cli -p ncut-desktop --all-targets --locked -- -D warnings
node --test OpenWrt/tests/portal.test.mjs
cd Desktop
npm ci
npm run build
npx playwright install chromium
npx playwright test
```

Protocol fixtures cover HTTP/JS redirects, cookie continuity, form encoding,
unrelated portals, cross-origin forms/POST redirects, bounded response size,
failed credentials and false success. Native Windows smoke checks use WebView2
CDP with synthetic UI inspection, not real credentials. Browser workflow tests
mock IPC and do not substitute for native checks. Campus/router hardware
testing is separate and must not be inferred from fixture results.

## GitHub Actions and release

`build.yml` builds every variant on push/PR/manual dispatch. Desktop installers
are NSIS `.exe`, universal macOS `.dmg`, Linux `.AppImage`. OpenWrt 25.12 is an
`all` architecture `.apk`; the workflow also publishes an `all` architecture
`.ipk` for OpenWrt 24.10 and earlier. Android is a signed universal `.apk`.

`v*` tags publish a release only when all build/verification jobs pass.
The release gate requires exactly five named installer files, with no archive
or CLI assets. CLI binaries and test screenshots stay in workflow artifacts.

Android signing uses `ANDROID_KEYSTORE_BASE64` and
`ANDROID_KEYSTORE_PASSWORD` repository secrets. Never rotate the key casually:
existing APK installations require the same signing identity for upgrades.
The authorized setup helper creates local recovery material under ignored
`.tools/android-signing/`. Windows/macOS public signing certificates are not
configured; packages use the platform's normal unsigned-app prompts.

## Protocol and migration

Probe `http://www.gstatic.com/generate_204`; only 204 is online. Validate the
NCUT title and Fortinet form; submit same-origin `4Tredir`, `magic`, `username`
and `password` with cookies. Keepalive must be followed by a successful probe.
Gateway hostnames/public IPs are supported. HTTPS validation stays enabled.
HTTP is the existing campus protocol and does not encrypt credentials; title
checks alone cannot authenticate a server. Use trusted campus networks.

Stop old WinSW/Startup, systemd/cron, launch agents and OpenWrt
`ncut_autologin` before migration. New OpenWrt service is `ncut-autologin`.
Old credentials are not imported. Historical source remains in Git history.
