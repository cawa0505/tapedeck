# 🎗️ 貢獻指南

感謝您對 tapedeck 的興趣！以下為貢獻流程規範：

## 📌 專案現況

- 目前僅支援 **Linux（Wayland）**：TUI 錄製轉譯給 [vhs](https://github.com/charmbracelet/vhs) 執行；GUI 側錄走 Wayland compositor IPC（niri / sway / umbriel）＋ wf-recorder
- X11 後端尚未實作，暫不接受相關 PR（歡迎先開 issue 討論）

## 🔧 開發環境設定

需求：Rust toolchain（edition 2021）、ffmpeg、vhs（TUI 模式）、wf-recorder（GUI 模式，Wayland）

```bash
git clone https://github.com/cawa0505/tapedeck.git
cd tapedeck
cargo build
cargo test
```

## 📌 貢獻原則

1. **規格先行（Documentation First）**：語法與行為變更需先更新 `openspec/specs/<change-set>/` 四件套（proposal / requirements / design / tasks），實作以文件為唯一依據。詳見 [AGENTS.md](AGENTS.md)
2. **引擎抽象**：新錄製後端須實作 `src/engine/dispatcher.rs` 的 `RecordingEngine` trait
3. **外部工具依賴走 adapter**：compositor 控制面（niri / sway / umbriel）、wf-recorder、ffmpeg 皆收斂在 adapter 層，不寫死於 dispatcher

## 💻 提交前檢查

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## 🔄 貢獻流程

1. Fork 本專案並建立 feature branch
2. 通過上述檢查後提交 PR，描述變更動機
3. PR 由 CI 自動執行 fmt / clippy / test 驗證

## 📦 發布流程（維護者）

1. 更新 `Cargo.toml` 版本號
2. 標記 Git Tag：`git tag vX.Y.Z && git push origin vX.Y.Z`

---

期待您的貢獻！
