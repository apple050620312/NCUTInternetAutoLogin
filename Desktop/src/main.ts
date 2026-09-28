import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { createIcons, Wifi, RefreshCw, Play, Pause, Eye, EyeOff, Save, Trash2, Network, ShieldCheck, CircleCheck, CircleAlert } from 'lucide';
import './style.css';
import logo from '../../Core/assets/app-icon.png';
import { statusLabels as labels, type NetworkStatus as Status } from '../../Core/web/status';

type Settings = { username: string; interval_seconds: number; auto_connect: boolean; start_at_login: boolean };
type Activity = { at: number; status: Status; message: string };
type Snapshot = { settings: Settings; status: Status; history: Activity[]; has_password: boolean };
let state: Snapshot = { settings: { username: '', interval_seconds: 15, auto_connect: false, start_at_login: false }, status: 'stopped', history: [], has_password: false };
let busy = false;
const app = document.querySelector<HTMLDivElement>('#app')!;
app.innerHTML = `
  <header><div class="brand"><img class="brand-mark" src="${logo}" alt="NCUT Logo"><div><strong>NCUT 校園網路</strong><span>勤益科技大學</span></div></div><span class="version">4.0</span></header>
  <main>
    <section class="connection" aria-labelledby="status-heading">
      <div class="section-label">連線狀態</div>
      <div class="status-row"><span id="status-icon" class="status-icon"><i data-lucide="wifi"></i></span><div><h1 id="status-heading" aria-live="polite"></h1><p id="status-detail"></p></div></div>
      <div class="connection-actions"><button id="connect" class="primary"><i data-lucide="play"></i>立即連線</button><button id="check"><i data-lucide="refresh-cw"></i>檢查連線</button><span id="monitor-label"></span></div>
    </section>
    <div class="workspace">
      <section class="settings" aria-labelledby="settings-heading">
        <h2 id="settings-heading">連線設定</h2>
        <form id="settings-form">
          <label for="username">校園帳號</label><input id="username" name="username" autocomplete="username" placeholder="s + 學號" maxlength="256" required />
          <label for="password">密碼</label><div class="password-field"><input id="password" name="password" type="password" autocomplete="current-password" /><button id="show-password" type="button" class="icon-button" title="顯示密碼" aria-label="顯示密碼"><i data-lucide="eye"></i></button></div>
          <div class="credential-note"><i data-lucide="shield-check"></i><span id="credential-label">密碼只保留在此裝置</span></div>
          <div class="numeric-setting"><label for="interval">檢查間隔</label><div><input id="interval" type="number" min="5" max="3600" step="1" value="15" required /><span>秒</span></div></div>
          <label class="toggle-row" for="auto-connect"><span>自動重新連線</span><input id="auto-connect" type="checkbox" role="switch" /></label>
          <label class="toggle-row" for="startup"><span>登入電腦時啟動</span><input id="startup" type="checkbox" role="switch" /></label>
          <div class="save-row"><button type="submit" id="save"><i data-lucide="save"></i>儲存設定</button><span id="save-state" role="status"></span></div>
        </form>
      </section>
      <section class="activity" aria-labelledby="activity-heading"><div class="activity-header"><h2 id="activity-heading">連線紀錄</h2><button id="clear" class="icon-button" title="清除紀錄" aria-label="清除紀錄"><i data-lucide="trash-2"></i></button></div><ol id="history"></ol><p id="empty">尚無連線紀錄</p></section>
    </div>
    <div id="notice" role="alert" hidden></div>
  </main>
  <footer><span><span class="dot"></span>NCUT Auto Login</span><button id="pause" class="text-button"><i data-lucide="pause"></i>暫停自動連線</button></footer>`;
function el<T extends HTMLElement = HTMLElement>(id: string) { return document.getElementById(id) as T; }
function icons() { createIcons({ icons: { Wifi, RefreshCw, Play, Pause, Eye, EyeOff, Save, Trash2, Network, ShieldCheck, CircleCheck, CircleAlert } }); }
function render(updateForm = false) {
  el('status-heading').textContent = labels[state.status];
  el('status-icon').className = `status-icon ${state.status}`;
  el('status-detail').textContent = state.status === 'online' ? '已通過網際網路連線檢查' : state.status === 'stopped' ? '可隨時檢查或連線' : state.status === 'unstable' ? '請確認 Wi-Fi 或網路線連接' : state.status === 'login_failed' ? '請確認校園帳號、密碼與網路' : state.status === 'missing_credentials' ? '儲存設定後即可連線' : '正在確認校園網路';
  el('monitor-label').textContent = state.settings.auto_connect ? `每 ${state.settings.interval_seconds} 秒自動檢查` : '自動檢查已關閉';
  el('credential-label').textContent = state.has_password ? '密碼已儲存於此裝置' : '密碼只保留在此裝置';
  el<HTMLInputElement>('password').placeholder = state.has_password ? '保留已儲存的密碼' : '校園網路密碼';
  if (updateForm) {
    el<HTMLInputElement>('username').value = state.settings.username;
    el<HTMLInputElement>('interval').value = String(state.settings.interval_seconds);
    el<HTMLInputElement>('auto-connect').checked = state.settings.auto_connect;
    el<HTMLInputElement>('startup').checked = state.settings.start_at_login;
    el<HTMLInputElement>('password').value = '';
  }
  el<HTMLButtonElement>('pause').disabled = busy || !state.settings.auto_connect;
  for (const id of ['connect', 'check', 'save']) el<HTMLButtonElement>(id).disabled = busy;
  el('history').replaceChildren(...state.history.slice().reverse().map(event => {
    const row = document.createElement('li');
    const indicator = document.createElement('span'); indicator.className = `event-dot ${event.status}`;
    const content = document.createElement('div');
    const message = document.createElement('span'); message.textContent = event.message;
    const time = document.createElement('time'); time.dateTime = new Date(event.at * 1000).toISOString(); time.textContent = new Date(event.at * 1000).toLocaleString('zh-TW', { hour12: false });
    content.append(message, time); row.append(indicator, content); return row;
  }));
  el('empty').hidden = state.history.length > 0;
}
async function command(name: string, args?: Record<string, unknown>, updateForm = false) {
  if (!isTauri()) { el('notice').hidden = false; el('notice').textContent = '無法連接，請稍後重試'; return; }
  busy = true; render(); el('notice').hidden = true;
  try { state = await invoke<Snapshot>(name, args); render(updateForm); }
  catch (error) { el('notice').textContent = typeof error === 'string' ? error : '操作未完成，請稍後重試'; el('notice').hidden = false; }
  finally { busy = false; render(); }
}
el('settings-form').addEventListener('submit', async event => {
  event.preventDefault();
  const settings: Settings = { username: el<HTMLInputElement>('username').value.trim(), interval_seconds: Number(el<HTMLInputElement>('interval').value), auto_connect: el<HTMLInputElement>('auto-connect').checked, start_at_login: el<HTMLInputElement>('startup').checked };
  const password = el<HTMLInputElement>('password').value || null;
  el('save-state').textContent = '';
  await command('save_settings', { settings, password }, true);
  if (el('notice').hidden) el('save-state').textContent = '已儲存';
});
el('show-password').addEventListener('click', () => {
  const input = el<HTMLInputElement>('password'); const hidden = input.type === 'password'; input.type = hidden ? 'text' : 'password';
  const button = el<HTMLButtonElement>('show-password'); button.title = hidden ? '隱藏密碼' : '顯示密碼'; button.setAttribute('aria-label', button.title); button.innerHTML = `<i data-lucide="${hidden ? 'eye-off' : 'eye'}"></i>`; icons();
});
el('connect').addEventListener('click', () => command('connect_now'));
el('check').addEventListener('click', () => command('check_now'));
el('pause').addEventListener('click', () => command('set_monitoring', { enabled: false }, true));
el('clear').addEventListener('click', () => command('clear_history'));
icons(); render(true);
if (isTauri()) {
  await listen<Snapshot>('network-status', event => { state = event.payload; render(); });
  await command('get_snapshot', undefined, true);
} else { el('notice').hidden = false; el('notice').textContent = '無法連接，請稍後重試'; }
