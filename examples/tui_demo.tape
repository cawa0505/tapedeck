# TUI 示範腳本：終端機應用程式錄製
Set Engine Auto
Set Output "assets/tui_demo.webm"
Set FPS 60

# 啟動 cargo 專案
Type "cargo run --example tui"
Sleep 500ms
Key Enter

# 模擬鍵盤導航
Key Down 3
Sleep 300ms
Key Tab
Sleep 200ms
Key Enter

# 結束錄製
Sleep 2s