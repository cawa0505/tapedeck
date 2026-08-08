# vhs serve（SSH 模式）事實表

> 來源：lib-3 研究（2026-08-08），vhs 官方 README + serve.go/serve_unix.go 原始碼。
> 目的：TUI 雙軌架構（design.md 4.1）vhs serve SSH 軌的實作依據。

## 機制

- `vhs serve` 內建 SSH server（charmbracelet/wish + ssh），非 HTTP
- client 端不需安裝 vhs/ffmpeg/待展示工具 — 全部在 serve 端執行
- 一次 SSH 連線 = 執行一份 tape = 回傳一個檔案，送完即斷

## 啟動與環境變數

| 變數 | 預設 | 說明 |
|---|---|---|
| `VHS_PORT` | `1976` | listen port |
| `VHS_HOST` | `localhost` | listen host |
| `VHS_KEY_PATH` | `.ssh/vhs_ed25519` | SSH host key（**相對 CWD**） |
| `VHS_AUTHORIZED_KEYS_PATH` | 空 | 空 = 完全無認證、任何人可連 |
| `VHS_UID` / `VHS_GID` | `0` | 兩者皆非 0 才 setuid/setgid 降權 |

## Client 連線

```
ssh host < demo.tape > demo.gif
```

- **stdin = tape 腳本內容**（serve 端讀完整個 stdin 後執行）
- **stdout = 產出檔案位元組**（gif/webm，`wish.Print` 原樣輸出，非 base64）
- **stderr = 錯誤/diagnostics**，失敗 exit 1
- **不接受 PTY**：帶 PTY 連線被拒（"PTY is not supported"、exit 1）
- 無密碼認證 — 只有 public key（`VHS_AUTHORIZED_KEYS_PATH`）或完全開放

## 與本機 `vhs tape.tape` 的差異

- 執行核心相同（serve 端直接 `Evaluate(...)`，tape 語言完全相容）
- **產物落在 client 端**：serve 忽略 tape 內 `Output` 路徑，改用 `os.TempDir()` 隨機暫存檔渲染，讀出位元組寫 stdout 後 `defer os.Remove` 刪除
- 格式選擇依 tape `Output` 副檔名：優先序 `mp4 > webm > gif`；多個 Output 只有最優先的回傳
- serve 版只支援 gif/mp4/webm 三種；本機版另有 `.txt`/`.ascii`/PNG frame sequence

## 已知限制

- 不支援密碼認證
- 多 session 支援（wish 並發 SSH server），無明確上限
- 不接受 PTY → 無法遠端互動式操作、無法 `vhs record`
- 單次連線單一輸出
- 無 HTTP/JSON API — 純 SSH + tape 文字協定 + 單一二進位回傳

## 使用情境

- **Headless 伺服器/遠端錄製**：serve 端裝好 vhs + ttyd + ffmpeg + 待展示 CLI 工具，client 零依賴送 tape 取 gif
- 官方 Docker 映像：`ghcr.io/charmbracelet/vhs`
- CI 錄製官方主路線是 local `vhs-action`；serve 適合「集中一台機器錄製、多 CI job 連線取檔」

## 對 Tapedeck 的意涵

- vhs serve 是「SSH + tape 文字協定 + 單一檔案回傳」，無狀態回報/多格式 API
- 若 Tapedeck 要 serve 軌，需在 vhs serve 之上自建層（session 管理、輸出收檔、錯誤解讀）
- 產物位元組經 stdout 回傳 → tapedeck 需自行寫入 XDG 路徑（與本機軌的 `resolve_output_path` 分流）
