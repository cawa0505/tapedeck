# 🎗️ 貢獻指南

感謝您對 tapedeck 的興趣！以下為貢獻流程規範：

## 📌 貢獻原則

1. **核心範圍優先**：
   - 目前僅支援 Linux (Wayland/X11)
   - 新功能需先通過 `src/engine/tape.rs` 的 RecordingEngine Trait 抽象
   - 非 Linux 平台請透過 PR 貢獻（需通過 CI 測試）

2. **開發流程**：
   - Fork 本專案並建立 feature branch
   - 提交 PR 前請執行 `cargo test` 和 `cargo clippy`
   - 更新 `CHANGELOG.md` 記錄變更

3. **代碼規範**：
   - 遵循 Rust 2024 風格指南
   - 每 80 字元換行
   - 使用 `anyhow` 錯誤處理

## 🔧 開發環境設定

```bash
# 安裝依賴
cargo install --locked

# 執行所有檢查
cargo xtask ci
```

## 📦 發布流程

1. 更新 `CHANGELOG.md`
2. 標記 Git Tag: `git tag vX.Y.Z`
3. 推送 Tag 觸發 GitHub Actions 發布至 crates.io

---

期待您的貢獻！