# uinput Rust crate 選擇調研（2026-08-08）

來源：crates.io API、GitHub API/原始碼、生產使用範例（rdev、wayvr、xremap、kanata、evdev-rs 官方 vmouse.rs）。調研任務：lib-4（ses_01f5f3767ffew00kaiAzUUkve7）。

## 結論

**採用 `evdev` crate（cmr/evdev）0.13.2** — 唯一活躍維護（2026-05 仍有 commit）、純 Rust 零系統依賴、1.4M downloads、高階 `VirtualDeviceBuilder` 直接滿足鍵盤+滑鼠需求，被 xremap/kanata/OpenLogi 等生產輸入工具採用。crates.io 上的 crate 名就是 `evdev`（不帶 -rs 後綴）。

## 三候選比較

| | evdev-rs | input-linux | **evdev（cmr/evdev）** |
|---|---|---|---|
| 最後 release | 0.6.3（2025-09-14） | 0.7.1（2024-08-19） | 0.13.2（2025-09-15） |
| 最後 commit | 2025-09-14 | 2024-08-19（>2 年休眠） | **2026-05-28** |
| downloads（90d） | 396,811 | 135,417 | **1,425,158** |
| uinput 高階 API | UInputDevice（低階，需手動 enable） | UInputHandle（低階 ioctl） | **VirtualDeviceBuilder（高階）** |
| SYN_REPORT | 手動 | 手動 | **emit() 自動** |
| 系統依賴 | **綁 libevdev（需 pkg-config / submodule 編譯）** | 無（手寫 kernel bindings） | **無（純 Rust）** |
| MSRV | 2018 | ~1.69（nix 0.29） | 2021 / **1.64 明確宣告** |

`input-linux` **不是** evdev-rs 的 fork — 兩者獨立實作（2016/2017），GitHub parent/source 皆 None。

## 事件送出 API（真實範例）

### evdev / cmr（官方 examples/virtual_keyboard.rs）
```rust
use evdev::{uinput::VirtualDevice, AttributeSet, KeyCode, KeyEvent};

let mut keys = AttributeSet::<KeyCode>::new();
keys.insert(KeyCode::KEY_A);
let mut device = VirtualDevice::builder()?
    .name("Fake Keyboard")
    .with_keys(&keys)?
    .build()?;

// key press（value 1）/ release（value 0）
device.emit(&[*KeyEvent::new(KeyCode(KeyCode::KEY_A.0), 1)])?;
device.emit(&[*KeyEvent::new(KeyCode(KeyCode::KEY_A.0), 0)])?;
```
`emit()` 自動補 SYN_REPORT。

### input-linux（rdev 生產實作 src/linux/wayland/simulate.rs）
```rust
use input_linux::{EventKind, InputId, Key, KeyEvent, KeyState, RelativeAxis,
    RelativeEvent, SynchronizeEvent, SynchronizeKind, UInputHandle};

let file = OpenOptions::new().write(true).custom_flags(O_NONBLOCK).open("/dev/uinput")?;
let uinput = UInputHandle::new(file);
uinput.set_evbit(EventKind::Key)?;
uinput.set_relbit(RelativeAxis::X)?;
uinput.set_relbit(RelativeAxis::Y)?;
uinput.set_keybit(Key::KeyA)?;      // 或 Key::ButtonLeft
uinput.create(&InputId { bustype: BUS_VIRTUAL, vendor: 0x1234, product: 0x5678, version: 1 },
    b"tapedeck virtual input", 0, &[])?;

// key/button: KeyEvent + 手動 SYN_REPORT
let ev: libc::input_event = InputEvent::from(KeyEvent::new(t, Key::KeyA, KeyState::PRESSED)).into();
let sync: libc::input_event = InputEvent::from(SynchronizeEvent::new(t, SynchronizeKind::Report, 0)).into();
handle.write(&[ev, sync])?;
```

### evdev-rs（官方 examples/vmouse.rs）
```rust
let u = UninitDevice::new()?;
u.set_name("Virtual Mouse");
u.set_bustype(BusType::BUS_USB as u16);
u.enable(EventCode::EV_KEY(EV_KEY::BTN_LEFT))?;  // 沒設按鍵會被當成不可用裝置
u.enable(EventCode::EV_REL(EV_REL::REL_X))?;
u.enable(EventCode::EV_REL(EV_REL::REL_Y))?;
let v = UInputDevice::create_from_device(&u)?;
v.write_event(&InputEvent { time, event_code: EventCode::EV_REL(EV_REL::REL_X), value: 10 })?;
v.write_event(&InputEvent { time, event_code: EventCode::EV_SYN(EV_SYN::SYN_REPORT), value: 0 })?; // 必須手動
```

## 陷阱清單

1. **權限**：/dev/uinput open 失敗 EACCES — 需 root 或 uinput group；部分發行版（Arch）需 `modprobe uinput`。handy-keys 用 `VirtualDevice::builder()` 預檢。
2. **滑鼠必須同時註冊 BTN 鍵 + REL 軸**，否則 libinput/桌面忽略該裝置（vmouse.rs 引 stackoverflow.com/a/64559658）。
3. **每批事件必須以 SYN_REPORT 結束**，否則 listeners 收不到（cmr/evdev 的 emit() 自動處理）。
4. **裝置隨 fd 銷毀**：關閉 fd = 裝置消失。必須把 handle 保活到程式結束（rdev 用 `static LazyLock<Mutex<Option<UInputHandle<File>>>>`）。
5. **O_NONBLOCK**：uinput buffer 滿時 blocking write 會卡；rdev/wayvr 都加 O_NONBLOCK。
6. **名稱上限**：UINPUT_MAX_NAME_SIZE = 80 bytes；cmr/evdev 超長直接 panic，名稱要先 truncate。
