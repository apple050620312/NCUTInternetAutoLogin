# 終端機版本

適用於僅有命令列的 Windows、Linux、macOS 裝置。
執行檔可以從 GitHub Actions 的 `cli-*` artifact 取得。

將 `NCUT_USERNAME` 與 `NCUT_PASSWORD` 設定為校園帳號密碼，再執行：

```sh
ncut check                # 查看目前連線
ncut login                # 立即登入
ncut run --interval 15    # 每 15 秒自動檢查與重連
```

密碼不接受命令列參數。`run` 可以用 Ctrl+C 結束，Linux 也接受 SIGTERM。
狀態變化時才會印出紀錄，避免持續洗版。

結束代碼：0 表示成功，2 表示尚未連線，1 表示設定或操作失敗。

Linux 開機常駐可使用本目錄的 `ncut.service`。
[開發、建置與服務設定](../docs/DEVELOPMENT.md)
