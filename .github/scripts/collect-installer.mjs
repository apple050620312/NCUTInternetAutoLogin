import { readdir, copyFile, mkdir, stat } from 'node:fs/promises';
import { join, resolve } from 'node:path';

const platform = process.argv[2];
const definitions = {
  windows: { root: 'target/release/bundle/nsis', suffix: '.exe', name: 'NCUT-Windows.exe' },
  macos: { root: 'target/universal-apple-darwin/release/bundle/dmg', suffix: '.dmg', name: 'NCUT-macOS.dmg' },
  linux: { root: 'target/release/bundle/appimage', suffix: '.AppImage', name: 'NCUT-Linux.AppImage' },
  'openwrt-apk': { root: 'bin/packages', suffix: '.apk', name: 'NCUT-OpenWrt.apk', prefix: 'ncut-autologin-' },
  'openwrt-ipk': { root: 'bin/packages', suffix: '.ipk', name: 'NCUT-OpenWrt.ipk', prefix: 'ncut-autologin_' },
  android: { root: 'Android/src-tauri/gen/android/app/build/outputs/apk', suffix: '.apk', name: 'unsigned-android.apk', prefix: 'app-universal-release' },
};
const definition = definitions[platform];
if (!definition) throw new Error('Unknown installer platform');
async function files(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  return (await Promise.all(entries.map(entry => entry.isDirectory() ? files(join(directory, entry.name)) : [join(directory, entry.name)]))).flat();
}
const matches = (await files(resolve(definition.root))).filter(path => path.endsWith(definition.suffix) && (!definition.prefix || path.split(/[\\/]/).at(-1).startsWith(definition.prefix)));
if (matches.length !== 1) throw new Error(`Expected one ${platform} installer, found ${matches.length}`);
if ((await stat(matches[0])).size === 0) throw new Error('Installer is empty');
const output = platform === 'android' ? '.tools' : 'release-assets';
await mkdir(output, { recursive: true });
await copyFile(matches[0], join(output, definition.name));
console.log(`Prepared ${definition.name}`);
