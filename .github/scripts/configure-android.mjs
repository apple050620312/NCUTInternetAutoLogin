import { readFile, writeFile } from 'node:fs/promises';
import { DOMParser, XMLSerializer } from '../../Android/node_modules/@xmldom/xmldom/lib/index.js';
const path = 'Android/src-tauri/gen/android/app/src/main/AndroidManifest.xml';
const doc = new DOMParser().parseFromString(await readFile(path, 'utf8'), 'application/xml');
const application = doc.getElementsByTagName('application')[0];
if (!application) throw new Error('Android application manifest is missing');
application.setAttributeNS('http://schemas.android.com/apk/res/android', 'android:allowBackup', 'false');
application.setAttributeNS('http://schemas.android.com/apk/res/android', 'android:usesCleartextTraffic', 'true');
await writeFile(path, new XMLSerializer().serializeToString(doc));
const gradlePath = 'Android/src-tauri/gen/android/app/build.gradle.kts';
let gradle = await readFile(gradlePath, 'utf8');
for (const setting of ['compileSdk', 'targetSdk']) {
  const pattern = new RegExp(`(${setting}\\s*=\\s*)\\d+`);
  if (!pattern.test(gradle)) throw new Error(`Generated Android ${setting} setting is missing`);
  gradle = gradle.replace(pattern, (_, prefix) => `${prefix}36`);
}
await writeFile(gradlePath, gradle);
