import { test, expect } from '@playwright/test';

test('mobile account, connection, pause and removal workflow', async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(window, 'isTauri', { value: true });
    let state = { username: '', interval: 30, enabled: false, status: 'stopped', hasPassword: false, history: [] as { at: number; status: string }[] };
    Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {
      invoke: async (_: string, args: { action: string; payload: { username?: string; password?: string; interval?: number } }) => {
        if (args.action === 'start' && !state.hasPassword) throw '請先儲存帳號與密碼';
        if (args.action === 'save') {
          if (!args.payload.password && !state.hasPassword) throw '請輸入密碼';
          state.username = args.payload.username!; state.interval = args.payload.interval!; state.hasPassword = true;
        }
        if (args.action === 'connect') { state.status = 'online'; state.history.push({ at: Date.now() / 1000, status: 'online' }); }
        if (args.action === 'start') state.enabled = true;
        if (args.action === 'stop') { state.enabled = false; state.status = 'stopped'; }
        if (args.action === 'forget') state = { username: '', interval: 30, enabled: false, status: 'stopped', hasPassword: false, history: [] };
        return structuredClone(state);
      },
    } });
  });
  await page.goto('/');
  const monitor = page.getByRole('switch', { name: '自動重新連線' });
  await monitor.click();
  await expect(page.getByRole('alert')).toHaveText('請先儲存帳號與密碼');
  await expect(monitor).not.toBeChecked();
  await page.getByLabel('帳號', { exact: true }).fill('s123456');
  await page.getByLabel('密碼', { exact: true }).fill('special&+secret');
  await page.getByRole('button', { name: '顯示密碼' }).click();
  await expect(page.getByLabel('密碼', { exact: true })).toHaveAttribute('type', 'text');
  await page.getByRole('button', { name: '隱藏密碼' }).click();
  await page.getByRole('button', { name: '儲存設定' }).click();
  await expect(page.getByRole('status')).toHaveText('已儲存');
  await expect(page.getByLabel('密碼', { exact: true })).toHaveValue('');
  await page.getByRole('button', { name: '立即連線' }).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('網路已連線');
  await monitor.check();
  await expect(page.getByText('每 30 秒檢查')).toBeVisible();
  for (const width of [600, 390, 320]) {
    await page.setViewportSize({ width, height: 860 });
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    expect(await page.locator('.brand-mark').evaluate((img: HTMLImageElement) => img.complete && img.naturalWidth > 0)).toBe(true);
    await page.screenshot({ path: `test-results/android-${width}.png`, fullPage: true });
  }
  await monitor.uncheck();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('自動連線已暫停');
  await page.getByRole('button', { name: '移除帳號', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: '取消' }).click();
  await expect(page.getByLabel('帳號', { exact: true })).toHaveValue('s123456');
  await page.getByRole('button', { name: '移除帳號', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: '移除帳號' }).click();
  await expect(page.getByLabel('帳號', { exact: true })).toHaveValue('');
  await expect(page.getByText('尚無連線紀錄')).toBeVisible();
});
