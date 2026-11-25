import { test } from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { mkdir, mkdtemp, writeFile, chmod, rm, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, delimiter } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn, spawnSync } from 'node:child_process';

const script = fileURLToPath(new URL('../files/ncut-autologin.sh', import.meta.url));
const windows = process.platform === 'win32';
const busybox = process.env.NCUT_TEST_SHELL === 'busybox';
const shell = busybox ? 'busybox' : windows ? 'C:/Program Files/Git/bin/sh.exe' : '/bin/sh';
const username = 's+中文&= 123';
const password = 'secret&=+"\'\\ 密碼';

async function putScript(bin, name, source) {
  const path = join(bin, name);
  await writeFile(path, '#!/bin/sh\n' + source + '\n');
  await chmod(path, 0o755);
}

async function fixture(mode, operation = 'login', options = {}) {
  let online = ['online', 'monitor', 'disabled', 'unconfirmed'].includes(mode);
  const gateway = ['direct', 'closed_post', 'monitor', 'disabled', 'unconfirmed', 'pin_origin',
    'gateway_offline', 'foreign_keepalive', 'foreign_redirect'].includes(mode);
  const campusForm = gateway || mode === 'campus_http' || mode === 'campus_js';
  const attempts = [];
  const clockAtLogin = [];
  const challengePaths = [];
  let keepalives = 0;
  let probeAttempts = 0;
  let fixtureError;
  const work = await mkdtemp(join(tmpdir(), 'ncut-fixture-'));
  const bin = join(work, 'bin');
  await mkdir(bin);
  const clock = join(work, 'clock');
  const delays = join(work, 'delays');
  await putScript(bin, 'uci', String.raw`
case "$*" in
    *.username) printf '%s' "$TEST_USERNAME" ;;
    *.password) printf '%s' "$TEST_PASSWORD" ;;
    *.interval) printf '%s' "$TEST_INTERVAL" ;;
    *.portal_url) printf '%s' "$TEST_GATEWAY" ;;
    *.renewal_minutes) printf '%s' "$TEST_RENEWAL" ;;
esac`);
  const paths = [bin];
  if (busybox) {
    const applets = join(work, 'applets');
    await mkdir(applets);
    const result = spawnSync('busybox', ['--install', '-s', applets]);
    assert.equal(result.status, 0, result.stderr.toString());
    paths.push(applets);
  }
  if (windows) {
    // Native curl needs Windows paths for file arguments embedded after '@'.
    await putScript(bin, 'curl', heredocCurl());
    await putScript(bin, 'hexdump', 'exec xxd -p -l 8 /dev/urandom');
    paths.push('C:/Program Files/Git/mingw64/bin');
  }
  if (operation === 'run') {
    await writeFile(clock, '0\n');
    await writeFile(delays, '');
    await putScript(bin, 'cut', 'cat "$TEST_CLOCK"');
    await putScript(bin, 'logger', 'exit 0');
    await putScript(bin, 'sleep', String.raw`
now=$(cat "$TEST_CLOCK")
if [ "$now" -ge "$TEST_STOP" ]; then
    kill -TERM "$PPID"
    exit 0
fi
printf '%s\n' "$1" >> "$TEST_DELAYS"
printf '%s\n' "$((now + $1))" > "$TEST_CLOCK"`);
  }

  const server = http.createServer(async (request, response) => {
    try {
      const origin = `http://127.0.0.1:${response.socket.localPort}`;
      const url = new URL(request.url, origin);
      if (url.pathname === '/probe') {
        probeAttempts += 1;
        if (mode === 'closed_post' && probeAttempts === 2) { response.writeHead(503).end(); return; }
        if (mode === 'monitor' && attempts.length === 1 && keepalives === 1 &&
            Number(await readFile(clock, 'utf8')) >= 65) online = false;
        if (online) { response.writeHead(204).end(); return; }
        if (mode === 'unrelated') { response.end('<title>Other network</title>'); return; }
        if (mode === 'direct' || mode === 'gateway_offline') { request.socket.destroy(); return; }
        if (gateway && mode !== 'pin_origin') { response.writeHead(503).end(); return; }
        const target = mode === 'pin_origin'
          ? `http://localhost:${response.socket.localPort}/fgtauth?url-token`
          : campusForm ? '/login?url-token' : '/fgtauth?url-token';
        if (mode === 'js' || mode === 'campus_js') {
          response.end(`<script>window.location.replace('${target}');</script>`);
        } else {
          response.writeHead(302, { Location: target }).end();
        }
        return;
      }
      if (url.pathname === '/fgtauth' || url.pathname === '/login') {
        challengePaths.push(request.url);
        if (mode === 'gateway_offline') { response.writeHead(503).end(); return; }
        const campus = url.pathname === '/login';
        if (campus && gateway) assert.match(url.search, /^\?[0-9a-f]{16}$/);
        const title = mode === 'wrong_title' ? 'Other network' : '國立勤益科技大學';
        const action = mode === 'foreign_form' ? 'http://example.com/steal' : campus ? '/' : '/auth';
        const token = campus ? `fixture-${challengePaths.length}` : 'token&amp;special';
        response.setHeader('Set-Cookie', 'session=fixture; Path=/');
        response.end(`<title>${title}</title>
          <form action='/unrelated'><input name='username'></form>
          <form action='${action}' method='post'>
            ${campus ? `<input type='hidden' name='4Tredir' value='${url.href}'>` : ''}
            <input value='${token}' name='magic' type='hidden'>
            <input name='username'><input name='password'>
          </form>`);
        return;
      }
      if (request.method === 'POST' && (url.pathname === '/auth' || url.pathname === '/')) {
        const formUrl = new URL(request.headers.referer);
        attempts.push(request.url);
        if (operation === 'run') clockAtLogin.push(Number(await readFile(clock, 'utf8')));
        let body = '';
        for await (const chunk of request) body += chunk;
        const form = new URLSearchParams(body);
        assert.equal(form.get('username'), username);
        assert.equal(form.get('password'), password);
        assert.equal(request.headers.origin, origin);
        assert.match(request.headers.cookie ?? '', /session=fixture/);
        if (url.pathname === '/') {
          assert.equal(form.get('magic'), `fixture-${challengePaths.length}`);
          assert.equal(form.get('4Tredir'), formUrl.href);
        } else {
          assert.equal(form.get('magic'), 'token&special');
          assert.equal(form.get('4Tredir'), `${origin}/probe`);
        }
        if (mode === 'reject') { response.end('invalid password'); return; }
        if (mode === 'post_redirect' || mode === 'foreign_redirect') {
          response.writeHead(307, { Location: `http://localhost:${response.socket.localPort}/steal` }).end();
          return;
        }
        online = mode !== 'false_success';
        if (mode === 'closed_post') { request.socket.destroy(); return; }
        if (mode === 'unconfirmed') { response.end('<title>請重新登入</title>'); return; }
        if (mode === 'foreign_keepalive') {
          response.end(`<script>location='http://localhost:${response.socket.localPort}/steal?session';</script>`);
          return;
        }
        response.end(`<script>window.location='/keepalive?session-${attempts.length}';</script>`);
        return;
      }
      if (url.pathname === '/keepalive') {
        assert.equal(request.method, 'GET');
        assert.equal(url.search, `?session-${attempts.length}`);
        keepalives += 1;
        response.end("<title>國立勤益科技大學</title>你已成功登入<a href='/logout?session-token'>登出</a>");
        return;
      }
      if (url.pathname === '/logout' || url.pathname === '/steal') {
        assert.fail(`unexpected ${url.pathname} request`);
      }
      response.writeHead(404).end();
    } catch (error) {
      fixtureError ??= error;
      response.writeHead(500).end();
    }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const url = `http://127.0.0.1:${server.address().port}`;
  const posix = path => path.replaceAll('\\', '/');
  try {
    const env = {
      ...process.env,
      NCUT_TEST_BIN: bin,
      TMPDIR: posix(work),
      NCUT_PROBE: `${url}/probe`,
      TEST_USERNAME: username,
      TEST_PASSWORD: password,
      TEST_INTERVAL: options.interval ?? '5',
      TEST_GATEWAY: gateway ? url : '',
      TEST_RENEWAL: options.renewal ?? '1',
      TEST_CLOCK: posix(clock),
      TEST_DELAYS: posix(delays),
      TEST_STOP: options.stop ?? '70',
    };
    const pathKey = Object.keys(env).find(key => key.toLowerCase() === 'path') || 'PATH';
    env[pathKey] = [...paths, env[pathKey]].join(delimiter);
    const args = [script, operation];
    if (busybox) args.unshift('sh');
    if (windows) args.unshift('-c', 'export PATH="$(cygpath -u "$NCUT_TEST_BIN"):$PATH"; exec /bin/sh "$@"', 'fixture');
    const child = spawn(shell, args, { env, windowsHide: true });
    const timer = setTimeout(() => child.kill(), 40_000);
    try {
      let output = '', error = '';
      child.stdout.on('data', chunk => output += chunk);
      child.stderr.on('data', chunk => error += chunk);
      const code = await new Promise((resolve, reject) => {
        child.on('error', reject);
        child.on('close', resolve);
      });
      assert.ifError(fixtureError);
      assert.equal(error, '');
      const sleepDelays = operation !== 'run' ? [] :
        (await readFile(delays, 'utf8')).trim().split('\n').filter(Boolean).map(Number);
      return { code, output: output.trim(), attempts, keepalives, challengePaths, clockAtLogin, sleepDelays, probeAttempts };
    } finally {
      clearTimeout(timer);
    }
  } finally {
    await new Promise(resolve => server.close(resolve));
    await rm(work, { recursive: true, force: true });
  }
}

function heredocCurl() {
  return [
    'for arg do',
    '  case "$arg" in *@/*) prefix=${arg%%@*}; path=${arg#*@}; arg="$prefix@$(cygpath -w "$path")" ;; esac',
    '  shift',
    '  set -- "$@" "$arg"',
    'done',
    "exec '/c/Program Files/Git/mingw64/bin/curl.exe' \"$@\"",
  ].join('\n');
}

test('OpenWrt accepts both portal paths and preserves the original challenge and cookies', async () => {
  for (const mode of ['http', 'js', 'campus_http', 'campus_js']) {
    const result = await fixture(mode);
    assert.equal(result.code, 0);
    assert.equal(result.output, 'online');
    assert.equal(result.challengePaths.length, 1);
    assert.equal(result.attempts.length, 1);
    assert.equal(result.keepalives, 1);
  }
});

test('OpenWrt uses the local gateway when the external probe cannot open a connection', async () => {
  for (const mode of ['direct', 'closed_post', 'pin_origin']) {
    const result = await fixture(mode);
    assert.equal(result.code, 0);
    assert.equal(result.output, 'online');
    assert.equal(result.challengePaths.filter(path => path.startsWith('/login?')).length, 1);
    assert.deepEqual(result.attempts, ['/']);
    assert.equal(result.keepalives, mode === 'closed_post' ? 0 : 1);
  }
});

test('OpenWrt refuses unrelated pages, cross-origin requests, and false success', async () => {
  for (const mode of ['unrelated', 'wrong_title', 'foreign_form', 'reject', 'false_success',
    'post_redirect', 'foreign_redirect', 'gateway_offline']) {
    const result = await fixture(mode);
    assert.equal(result.code, 1);
    assert.equal(result.output, ['unrelated', 'gateway_offline'].includes(mode) ? 'unstable' : 'login_failed');
    if (['unrelated', 'wrong_title', 'foreign_form', 'gateway_offline'].includes(mode)) {
      assert.deepEqual(result.attempts, []);
    }
  }
});

test('OpenWrt ignores a keepalive destination on another origin', async () => {
  const result = await fixture('foreign_keepalive');
  assert.equal(result.output, 'online');
  assert.equal(result.keepalives, 0);
});

test('OpenWrt one-shot check never submits credentials', async () => {
  for (const [mode, code, status] of [
    ['http', 1, 'needs_login'], ['online', 0, 'online'], ['direct', 1, 'unstable'],
  ]) {
    const result = await fixture(mode, 'check');
    assert.equal(result.code, code);
    assert.equal(result.output, status);
    assert.deepEqual(result.attempts, []);
  }
});

test('OpenWrt renews on schedule and recovers when the monitored connection drops', async () => {
  const result = await fixture('monitor', 'run', { interval: '17', stop: '94' });
  assert.equal(result.code, 0);
  assert.equal(result.output, 'online\nrenewed');
  assert.deepEqual(result.clockAtLogin, [60, 77]);
  assert.deepEqual(result.sleepDelays, [17, 17, 17, 9, 17, 17]);
  assert.equal(result.keepalives, 2);
  assert.notEqual(result.challengePaths[0], result.challengePaths[1]);
});

test('OpenWrt reports an unconfirmed online renewal and leaves a disabled renewal idle', async () => {
  const unconfirmed = await fixture('unconfirmed', 'run', { interval: '60', stop: '60' });
  assert.equal(unconfirmed.code, 0);
  assert.equal(unconfirmed.output, 'online\nrenewal_unconfirmed');
  assert.deepEqual(unconfirmed.clockAtLogin, [60]);
  const disabled = await fixture('disabled', 'run', { interval: '120', renewal: '0', stop: '120' });
  assert.equal(disabled.code, 0);
  assert.equal(disabled.output, 'online');
  assert.deepEqual(disabled.attempts, []);
});
