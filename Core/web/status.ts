export type NetworkStatus = 'online' | 'needs_login' | 'unstable' | 'checking' | 'authenticating' | 'stopped' | 'missing_credentials' | 'login_failed';
export const statusLabels: Record<NetworkStatus, string> = {
  online: '網路已連線', needs_login: '等待校園登入', unstable: '等待網路連線',
  checking: '正在檢查連線', authenticating: '正在登入', stopped: '自動連線已暫停',
  missing_credentials: '尚未設定帳號', login_failed: '登入未完成',
};
