# Refactor plan

## Scope and order

1. Windows desktop usability, native app and direct .exe installer.
2. OpenWrt shell service and direct architecture-independent .ipk package.
3. Linux desktop and direct .AppImage installer.
4. Android Tauri app with foreground reconnect service, Keystore credentials
   and a consistently signed universal .apk.
5. macOS desktop and universal .dmg installer.

Shared implementation: Rust workspace for Fortinet engine and configuration;
Tauri 2 desktop app for Windows, macOS, Linux: Traditional Chinese UI,
   status, credential setup, immediate check, automatic reconnect, tray,
   login startup, bounded activity history and OS credential storage.
OpenWrt: POSIX shell/curl, UCI configuration, procd service, package and
   local installer; no Python or Rust runtime needed on routers.
Headless Rust CLI: one-shot status/login and interruptible daemon,
   environment credentials, systemd example.
Protocol fixtures, build checks, desktop UI checks, CI across desktops,
   documentation and legacy migration.

## Protocol requirements from existing implementation

- Probe http://www.gstatic.com/generate_204; only 204 means online.
- Recognize HTTP redirects and window.location Fortinet redirects.
- Preserve cookies while visiting /fgtauth?<magic> and submitting credentials.
- Submit 4Tredir, magic, username, password as URL encoded form data.
- A keepalive marker alone is not proof of Internet access: verify with probe.
- Preserve scheme, host and port from portal; never disable TLS validation.
- Validate NCUT title, Fortinet form and same-origin form submission.
  Support public IPs and hostnames as in the existing implementation.
- Do not forward credential POST requests across origins on redirect.
- Bound request duration, redirect count, response size and retry frequency.
- Do not display credentials, portal tokens or response bodies in logs.

## Verification and limits

Use local HTTP fixture servers for protocol behavior, failures, special
characters and cookie continuity. Build desktop packages on platform runners.
Test browser UI at desktop/mobile widths. Test shell with mocked curl and
BusyBox when available. Live campus login requires campus network access and
real credentials, and must not be claimed from fixture evidence.

Android is now in scope. iOS is excluded. Keep the core independent from
desktop and OS storage. Shared logo is from the user-specified AI LIFE project.
GitHub Actions builds all installers; tag releases contain exactly five raw
installer assets, one per platform, without archive assets. CLI builds remain
workflow artifacts. User documentation prioritizes installation and experience.

## Progress

- [x] Inspect existing Python and OpenWrt implementations.
- [x] Shared Rust core and CLI; protocol fixtures passed locally.
- [x] Windows native app, IPC and NSIS installer passed locally.
- [x] Desktop responsive UI workflows/screenshots passed locally.
- [x] OpenWrt shell protocol fixtures passed locally.
- [ ] OpenWrt SDK package build.
- [ ] Linux and macOS native installers.
- [ ] Android native build, signed APK and runtime checks.
- [ ] Cross-platform CI, documentation and verification.
