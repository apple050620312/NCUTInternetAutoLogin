import { test } from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { mkdtemp, writeFile, chmod, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, delimiter } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn, spawnSync } from 'node:child_process';

const script = fileURLToPath(new URL('../files/ncut-autologin.sh', import.meta.url));
const shell = process.env.NCUT_TEST_SHELL === 'busybox' ? 'busybox' : process.platform === 'win32' ? 'C:/Program Files/Git/bin/sh.exe' : '/bin/sh';
const username = 's+中文&= 123';
const password = 'secret&=+"\'\\ 密碼';

async function fixture(mode, operation = 'login') {
  let online = false;
  let submitted = false;
  const work = await mkdtemp(join(tmpdir(), 'ncut-fixture-'));
  await writeFile(join(work, 'uci'), '#!/bin/sh\ncase "$*" in *username) printf "%s" "$TEST_USERNAME";; *password) printf "%s" "$TEST_PASSWORD";; *interval) echo 15;; esac\n');
  await chmod(join(work, 'uci'), 0o755);
  if (shell === 'busybox') {
    const result = spawnSync('busybox', ['--install', '-s', work]);
    assert.equal(result.status, 0, result.stderr.toString());
  }
  if (process.platform === 'win32') {
    // Native Windows curl does not understand MSYS paths embedded after '@'.
    await writeFile(join(work, 'curl'), `#!/bin/sh
for arg do
  case "$arg" in *@/*) prefix=\${arg%%@*}; path=\${arg#*@}; arg="$prefix@$(cygpath -w "$path")" ;; esac
  shift
  set -- "$@" "$arg"
done
exec '/c/Program Files/Git/mingw64/bin/curl.exe' "$@"
`);
    await chmod(join(work, 'curl'), 0o755);
  }
  const server = http.createServer(async (req, res) => {
    if (req.url === '/probe') {
      if (online || mode === 'online') { res.writeHead(204); res.end(); return; }
      if (mode === 'unrelated') { res.end('<title>Other network</title>'); return; }
      if (mode === 'js') { res.end("<script>window.location='/fgtauth?url-token';</script>"); return; }
      res.writeHead(302, { Location: '/fgtauth?url-token' }); res.end(); return;
    }
    if (req.url?.startsWith('/fgtauth?')) {
      const title = mode === 'wrong_title' ? 'Other network' : '勤益科技大學';
      const action = mode === 'foreign_form' ? 'http://example.com/steal' : '/auth';
      res.setHeader('Set-Cookie', 'session=fixture; Path=/');
      res.end(`<title>${title}</title><form action='${action}'><input name='username'><input name='password'><input name='magic' value='token&amp;special'></form>`); return;
    }
    if (req.url === '/auth') {
      submitted = true;
      let body = ''; for await (const chunk of req) body += chunk;
      const form = new URLSearchParams(body);
      assert.equal(form.get('username'), username);
      assert.equal(form.get('password'), password);
      assert.equal(form.get('magic'), 'token&special');
      assert.ok(req.headers.cookie?.includes('session=fixture'));
      if (mode === 'reject') { res.end('invalid password'); return; }
      if (mode === 'post_redirect') { res.writeHead(307, { Location: 'http://example.com/steal' }); res.end(); return; }
      online = mode !== 'false_success';
      res.end("<a href='/keepalive?token'>Connected</a>"); return;
    }
    res.writeHead(404); res.end();
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  try {
    const env = { ...process.env, NCUT_TEST_BIN: work, TMPDIR: work.replaceAll('\\', '/'), NCUT_PROBE: `http://127.0.0.1:${address.port}/probe`, TEST_USERNAME: username, TEST_PASSWORD: password };
    const pathKey = Object.keys(env).find(key => key.toLowerCase() === 'path') || 'PATH';
    const gitCurl = process.platform === 'win32' ? `C:/Program Files/Git/mingw64/bin${delimiter}` : '';
    env[pathKey] = `${work}${delimiter}${gitCurl}${env[pathKey]}`;
    const args = [script, operation];
    if (shell === 'busybox') args.unshift('sh');
    if (process.platform === 'win32') args.unshift('-c', 'export PATH="$(cygpath -u "$NCUT_TEST_BIN"):$PATH"; exec /bin/sh "$@"', 'fixture');
    const child = spawn(shell, args, { env, windowsHide: true });
    let output = '', error = '';
    child.stdout.on('data', chunk => output += chunk);
    child.stderr.on('data', chunk => error += chunk);
    const code = await new Promise((resolve, reject) => { child.on('error', reject); child.on('close', resolve); });
    assert.equal(error, '');
    return { code, output: output.trim(), submitted };
  } finally {
    await new Promise(resolve => server.close(resolve));
    await rm(work, { recursive: true, force: true });
  }
}

test('OpenWrt HTTP/JS redirect, cookies, special characters and verification', async () => {
  for (const mode of ['http', 'js']) {
    const result = await fixture(mode);
    assert.deepEqual(result, { code: 0, output: 'online', submitted: true });
  }
});
test('OpenWrt refuses unrelated pages and false success', async () => {
  for (const mode of ['unrelated', 'wrong_title', 'foreign_form', 'reject', 'false_success', 'post_redirect']) {
    const result = await fixture(mode);
    assert.equal(result.code, 1);
    assert.equal(result.output, mode === 'unrelated' ? 'unstable' : 'login_failed');
    if (['unrelated', 'wrong_title', 'foreign_form'].includes(mode)) assert.equal(result.submitted, false);
  }
});
test('OpenWrt one-shot check does not submit credentials', async () => {
  assert.deepEqual(await fixture('http', 'check'), { code: 1, output: 'needs_login', submitted: false });
  assert.deepEqual(await fixture('online', 'check'), { code: 0, output: 'online', submitted: false });
});
