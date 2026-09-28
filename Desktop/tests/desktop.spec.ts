import { test, expect } from '@playwright/test';

test('browser preview reports absent desktop connection', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('alert')).toHaveText('無法連接，請稍後重試');
  await page.getByLabel('校園帳號').fill('s123');
  await page.getByLabel('密碼', { exact: true }).fill('private&+');
  await page.getByRole('button', { name: '顯示密碼' }).click();
  await expect(page.getByLabel('密碼', { exact: true })).toHaveAttribute('type', 'text');
  await page.getByRole('button', { name: '隱藏密碼' }).click();
  await expect(page.getByLabel('密碼', { exact: true })).toHaveAttribute('type', 'password');
  await page.getByRole('button', { name: '立即連線' }).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('自動連線已暫停');
});

test('desktop IPC workflow and responsive layout', async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(window, 'isTauri', { value: true });
    let state = { settings: { username: '', interval_seconds: 15, auto_connect: false, start_at_login: false }, status: 'stopped', history: [] as { at: number; status: string; message: string }[], has_password: false };
    Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {
      transformCallback: () => 1,
      invoke: async (name: string, args: { settings?: typeof state.settings; password?: string; enabled?: boolean }) => {
        if (name === 'plugin:event|listen') return 1;
        if (name === 'save_settings') { state.settings = args.settings!; state.has_password = !!args.password; }
        if (name === 'connect_now') { state.status = 'online'; state.history.push({ at: Math.floor(Date.now() / 1000), status: 'online', message: '網路連線正常' }); }
        if (name === 'set_monitoring') { state.settings.auto_connect = args.enabled!; state.status = 'stopped'; }
        if (name === 'clear_history') state.history = [];
        return JSON.parse(JSON.stringify(state));
      },
    } });
  });
  await page.goto('/');
  await page.getByLabel('校園帳號').fill('s123456');
  await page.getByLabel('密碼', { exact: true }).fill('secret&+');
  await page.getByRole('switch', { name: '自動重新連線' }).check();
  await page.getByRole('button', { name: '儲存設定' }).click();
  await expect(page.getByRole('status')).toHaveText('已儲存');
  await expect(page.getByLabel('密碼', { exact: true })).toHaveValue('');
  await page.getByRole('button', { name: '立即連線' }).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('網路已連線');
  await expect(page.getByText('網路連線正常', { exact: true })).toBeVisible();
  for (const width of [900, 420, 360]) {
    await page.setViewportSize({ width, height: 900 });
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    await expect(page.getByRole('button', { name: '儲存設定' })).toBeVisible();
    await page.screenshot({ path: `test-results/desktop-${width}.png`, fullPage: true });
  }
  await page.getByRole('button', { name: '暫停自動連線' }).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('自動連線已暫停');
  await page.getByRole('button', { name: '清除紀錄' }).click();
  await expect(page.getByText('尚無連線紀錄')).toBeVisible();
});
