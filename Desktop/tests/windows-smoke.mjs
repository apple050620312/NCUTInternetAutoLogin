import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { resolve } from 'node:path';
import { mkdir } from 'node:fs/promises';
import assert from 'node:assert/strict';

if (process.platform !== 'win32') throw new Error('Windows WebView2 smoke check only');
const child = spawn(resolve('../target/debug/ncut-desktop.exe'), [], { windowsHide: true, env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=9223' } });
let browser;
try {
  const deadline = Date.now() + 30_000;
  while (!browser && Date.now() < deadline) {
    try { browser = await chromium.connectOverCDP('http://127.0.0.1:9223'); }
    catch { if (child.exitCode !== null) throw new Error(`App exited: ${child.exitCode}`); await new Promise(resolve => setTimeout(resolve, 250)); }
  }
  if (!browser) throw new Error('Windows WebView2 did not start');
  const context = browser.contexts()[0];
  let page;
  for (let i = 0; i < 40; i++) {
    page = context.pages().find(page => !page.url().startsWith('devtools:'));
    if (page) break;
    await new Promise(resolve => setTimeout(resolve, 250));
  }
  if (!page) throw new Error('No app window');
  await page.getByRole('heading', { level: 1 }).waitFor();
  assert.equal(await page.getByRole('alert').isVisible(), false);
  assert.equal(await page.getByRole('img', { name: 'NCUT Logo' }).evaluate(image => image.complete && image.naturalWidth > 0), true);
  await page.getByRole('button', { name: '檢查連線' }).click();
  await page.getByRole('button', { name: '檢查連線' }).waitFor({ state: 'visible' });
  await page.waitForFunction(() => !document.getElementById('check').disabled, undefined, { timeout: 30_000 });
  assert.equal(await page.getByRole('alert').isVisible(), false);
  await mkdir('test-results', { recursive: true });
  await page.screenshot({ path: 'test-results/windows-native.png', fullPage: true });
  console.log(`Windows native window and IPC passed: ${await page.getByRole('heading', { level: 1 }).textContent()}`);
} finally {
  if (browser) await browser.close();
  child.kill();
}
