import { invoke, isTauri } from '@tauri-apps/api/core';
import { createIcons, Wifi, Network, Play, Pause, RefreshCw, Eye, EyeOff, Save, Trash2 } from 'lucide';
import './style.css';
import logo from '../../Core/assets/app-icon.png';
import { statusLabels as labels, type NetworkStatus } from '../../Core/web/status';

type Snapshot = { username: string; interval: number; enabled: boolean; status: NetworkStatus; hasPassword: boolean; history: { at: number; status: NetworkStatus }[] };
let state: Snapshot = { username: '', interval: 30, enabled: false, status: 'stopped', hasPassword: false, history: [] };
let busy = false;
let requestVersion = 0;
document.querySelector('#app')!.innerHTML = `
<header><img class="brand-mark" src="${logo}" alt="NCUT Logo"><div><strong>NCUT 校園網路</strong><span>勤益科技大學</span></div></header>
<main><section class="connection"><span id="status-icon"><i data-lucide="wifi"></i></span><h1 id="status" aria-live="polite"></h1><p id="detail"></p><div class="actions"><button id="connect" class="primary"><i data-lucide="play"></i>立即連線</button><button id="check" class="icon-button" title="檢查連線" aria-label="檢查連線"><i data-lucide="refresh-cw"></i></button></div></section>
<section class="monitor"><div><strong>自動重新連線</strong><span id="monitor-label"></span></div><input id="monitor" type="checkbox" role="switch" aria-label="自動重新連線"></section>
<section class="settings"><h2>校園帳號</h2><form id="form"><label for="username">帳號</label><input id="username" autocomplete="username" placeholder="s + 學號" maxlength="256" required><label for="password">密碼</label><div class="password-field"><input id="password" type="password" autocomplete="current-password"><button id="eye" type="button" class="icon-button" title="顯示密碼" aria-label="顯示密碼"><i data-lucide="eye"></i></button></div><div class="interval"><label for="interval">檢查間隔</label><div><input id="interval" type="number" min="15" max="3600" value="30" required><span>秒</span></div></div><div class="save-row"><button id="save" type="submit"><i data-lucide="save"></i>儲存設定</button><span id="saved" role="status"></span><button id="forget" type="button" class="icon-button" title="移除帳號" aria-label="移除帳號"><i data-lucide="trash-2"></i></button></div></form></section>
<section class="activity"><h2>連線紀錄</h2><ol id="history"></ol><p id="empty">尚無連線紀錄</p></section><p id="notice" role="alert" hidden></p></main>
<dialog id="confirm"><h2>移除已儲存的帳號？</h2><p>自動連線將會暫停。</p><div><button id="cancel">取消</button><button id="confirm-forget">移除帳號</button></div></dialog>`;
function el<T extends HTMLElement = HTMLElement>(id: string) { return document.getElementById(id) as T; }
function icons() { createIcons({ icons: { Wifi, Network, Play, Pause, RefreshCw, Eye, EyeOff, Save, Trash2 } }); }
function render(form = false) {
  el('status').textContent = labels[state.status] || '等待網路連線';
  el('status-icon').className = state.status;
  el('detail').textContent = state.status === 'online' ? '已連上網際網路' : state.status === 'login_failed' ? '請確認帳號、密碼與校園網路' : state.status === 'unstable' ? '請確認 Wi-Fi 或行動網路' : state.username || '尚未設定校園帳號';
  el('monitor-label').textContent = state.enabled ? `每 ${state.interval} 秒檢查` : '已暫停';
  el<HTMLInputElement>('monitor').checked = state.enabled;
  el<HTMLInputElement>('monitor').disabled = busy;
  el<HTMLInputElement>('password').placeholder = state.hasPassword ? '保留已儲存的密碼' : '校園網路密碼';
  if (form) { el<HTMLInputElement>('username').value = state.username; el<HTMLInputElement>('interval').value = String(state.interval); el<HTMLInputElement>('password').value = ''; }
  for (const id of ['save', 'connect', 'check', 'forget']) el<HTMLButtonElement>(id).disabled = busy;
  el('history').replaceChildren(...state.history.slice(-20).reverse().map(event => {
    const row = document.createElement('li'); const text = document.createElement('span'); text.textContent = labels[event.status] || '等待網路連線'; const time = document.createElement('time'); time.textContent = new Date(event.at * 1000).toLocaleString('zh-TW', { hour12: false }); row.append(text, time); return row;
  }));
  el('empty').hidden = state.history.length > 0;
}
async function action(name: string, payload: Record<string, unknown> = {}, form = false, silent = false) {
  if (!isTauri()) { if (!silent) { el('notice').hidden = false; el('notice').textContent = '無法連接，請稍後重試'; } return; }
  const version = silent ? requestVersion : ++requestVersion;
  if (!silent) { busy = true; render(); el('notice').hidden = true; }
  try {
    const snapshot = await invoke<Snapshot>('plugin:ncut-mobile|execute', { action: name, payload });
    if (version === requestVersion && (!silent || !busy)) { state = snapshot; render(form); }
  }
  catch (error) { if (!silent) { el('notice').hidden = false; el('notice').textContent = typeof error === 'string' ? error : '操作未完成，請稍後重試'; } }
  finally { if (!silent) { busy = false; render(); } }
}
el('form').addEventListener('submit', async event => { event.preventDefault(); el('saved').textContent = ''; await action('save', { username: el<HTMLInputElement>('username').value, password: el<HTMLInputElement>('password').value || null, interval: Number(el<HTMLInputElement>('interval').value) }, true); if (el('notice').hidden) el('saved').textContent = '已儲存'; });
el('connect').addEventListener('click', () => action('connect'));
el('check').addEventListener('click', () => action('check'));
el('monitor').addEventListener('change', () => action(el<HTMLInputElement>('monitor').checked ? 'start' : 'stop'));
el('eye').addEventListener('click', () => { const input = el<HTMLInputElement>('password'); const show = input.type === 'password'; input.type = show ? 'text' : 'password'; const button = el<HTMLButtonElement>('eye'); button.title = show ? '隱藏密碼' : '顯示密碼'; button.setAttribute('aria-label', button.title); button.innerHTML = `<i data-lucide="${show ? 'eye-off' : 'eye'}"></i>`; icons(); });
el('forget').addEventListener('click', () => el<HTMLDialogElement>('confirm').showModal());
el('cancel').addEventListener('click', () => el<HTMLDialogElement>('confirm').close());
el('confirm-forget').addEventListener('click', () => { el<HTMLDialogElement>('confirm').close(); return action('forget', {}, true); });
icons(); render(true); await action('snapshot', {}, true, true);
setInterval(() => { if (!busy && !document.hidden) void action('snapshot', {}, false, true); }, 3000);
document.addEventListener('visibilitychange', () => { if (!document.hidden && !busy) void action('snapshot', {}, false, true); });
