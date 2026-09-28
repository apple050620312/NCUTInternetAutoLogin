import { readdir, stat } from 'node:fs/promises';
const expected = ['NCUT-Windows.exe', 'NCUT-OpenWrt.apk', 'NCUT-OpenWrt.ipk', 'NCUT-Linux.AppImage', 'NCUT-Android.apk', 'NCUT-macOS.dmg'].sort();
const files = (await readdir('release-assets')).sort();
if (JSON.stringify(files) !== JSON.stringify(expected)) throw new Error(`Release must contain exactly five installers. Found: ${files.join(', ')}`);
for (const file of files) {
  const info = await stat(`release-assets/${file}`);
  if (!info.isFile() || info.size === 0) throw new Error(`Invalid installer: ${file}`);
}
console.log('Verified one directly installable file per platform.');
