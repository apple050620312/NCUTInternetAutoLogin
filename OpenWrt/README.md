# OpenWrt 使用說明

## 安裝與設定

1. 下載 `NCUT-OpenWrt.ipk`。
2. 在路由器管理頁面「系統 → 軟體」上傳並安裝。
3. 重新登入管理頁面，開啟「網路 → 校園網路」。
4. 填寫校園帳號與密碼，開啟「自動重新連線」，儲存並套用。

頁面可以查看連線狀態與立即登入。預設每 15 秒檢查一次；連線失敗時會逐步延長
等待時間，恢復後回到設定的檢查間隔。路由器重新開機後會繼續自動連線。

安裝時如果提示缺少套件，請先安裝 `curl` 與 `ca-bundle`。
安裝包適用於不同 CPU 架構的 OpenWrt 23.05 / 24.10 路由器。
新版使用 apk 套件的 OpenWrt 請使用下面的本地安裝方式。

## 沒有網頁管理介面的路由器

在 `/etc/config/ncut-autologin` 設定 `enabled '1'`、`username`、`password`
與 `interval`，然後執行：

```sh
chmod 600 /etc/config/ncut-autologin
/etc/init.d/ncut-autologin enable
/etc/init.d/ncut-autologin restart
```

```sh
ncut-autologin check       # 查看目前連線
ncut-autologin login       # 立即登入
logread -e ncut-autologin   # 查看紀錄
```

本地安裝：將此目錄複製至路由器，以 root 執行 `sh install.sh`。

## 更新與移除

更新安裝包時會保留帳號設定。從舊版換到新版前，請停用舊服務
`/etc/init.d/ncut_autologin`，避免重複登入。

移除前先關閉自動連線，再從「系統 → 軟體」解除安裝。
需要清除帳號密碼時，一併刪除 `/etc/config/ncut-autologin`。

[開發與建置](../docs/DEVELOPMENT.md)
