import { randomBytes } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { encoding: 'utf8', windowsHide: true, ...options });
  if (result.status !== 0) throw new Error(`${command} failed: ${result.stderr || 'see build environment'}`);
  return result.stdout;
}
const repository = 'apple050620312/NCUTInternetAutoLogin';
const secrets = JSON.parse(run('gh', ['secret', 'list', '--repo', repository, '--json', 'name']));
if (secrets.some(secret => ['ANDROID_KEYSTORE_BASE64', 'ANDROID_KEYSTORE_PASSWORD'].includes(secret.name))) {
  if (!['ANDROID_KEYSTORE_BASE64', 'ANDROID_KEYSTORE_PASSWORD'].every(name => secrets.some(secret => secret.name === name))) throw new Error('Android signing is partially configured; preserve the existing key.');
  console.log('Android signing is already configured; existing key preserved.');
} else {
  const directory = resolve('.tools/android-signing');
  await mkdir(directory, { recursive: true });
  const keyPath = resolve(directory, 'release.jks');
  let password;
  try { password = JSON.parse(await readFile(resolve(directory, 'credentials.json'), 'utf8')).password; }
  catch {
    password = randomBytes(32).toString('hex');
    run('keytool', ['-genkeypair', '-keystore', keyPath, '-alias', 'ncut', '-keyalg', 'RSA', '-keysize', '3072', '-validity', '10000', '-dname', 'CN=NCUT Auto Login,O=sangege,C=TW', '-storepass:env', 'NCUT_SIGNING_PASSWORD', '-keypass:env', 'NCUT_SIGNING_PASSWORD'], { env: { ...process.env, NCUT_SIGNING_PASSWORD: password } });
    await writeFile(resolve(directory, 'credentials.json'), JSON.stringify({ alias: 'ncut', password }), { mode: 0o600 });
  }
  const encoded = (await readFile(keyPath)).toString('base64');
  run('gh', ['secret', 'set', 'ANDROID_KEYSTORE_BASE64', '--repo', repository], { input: encoded });
  run('gh', ['secret', 'set', 'ANDROID_KEYSTORE_PASSWORD', '--repo', repository], { input: password });
  console.log('Android signing key configured in GitHub Actions secrets. Local recovery files are in .tools/android-signing (ignored by Git).');
}
