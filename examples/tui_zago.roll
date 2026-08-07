# zago TUI 測試腳本 [.tape]
# 目標：驗證 TUI 介面在 fzf 模式下的鍵盤導航與預覽功能

Set Engine Auto
Set Output "assets/zago_tui_test.webm"
Set FPS 30

# 1. 啟動測試介面
Type "zago"
Sleep 1s
Enter

# 2. 鍵盤導航測試
Sleep 500ms
Key Down
Sleep 200ms
Key Down
Sleep 200ms

# 3. 選擇並觸發操作
Key Enter
Sleep 1s

# 4. 模擬滑鼠點擊 (若支援)
Sleep 300ms
Key Tab
Sleep 300ms
Key Enter

# 5. 結束測試
Sleep 1s
Key q  # 退出 TUI
