# Serde 未知欄位容錯：實測結論

## 結論（2026-08-08 實測驗證）

**serde 預設就會忽略未知欄位** — 不需要任何 `#[serde(...)]` 屬性。

最小重現（serde 1.0.x + derive）：

```rust
#[derive(Debug, Deserialize)]
struct Foo { x: i32 }

let v = serde_json::json!({"x": 42, "y": "unknown"});
serde_json::from_value::<Foo>(v)  // → Ok(Foo { x: 42 }) ✅
```

## 為什麼 `#[serde(ignore_unknown_fields)]` 會編譯失敗

- 原始碼驗證（`serde_derive_internals` symbol.rs）：屬性表只有 `DENY_UNKNOWN_FIELDS`，**沒有 `ignore_unknown_fields` symbol**
- serde_derive 1.0.219 / 1.0.228 / 1.0.229 三個版本一致 — 該屬性從未存在於這些版本
- 報錯：`error: unknown serde container attribute 'ignore_unknown_fields'`

## 背景：這是 feature request 誤讀

- [serde issue #44](https://github.com/serde-rs/serde/issues/44) 是 2015 年的 **feature request**（「allow struct deserialization to ignore unknown fields」），標籤 enhancement
- 該功能實際上是 serde 的**預設行為**，因此該屬性從未成為正式屬性
- 先前調研（librarian ses_021d46c9fffeNzDKNOMkcoAhTV）將其誤讀為「1.0.228 移除的 regression」，並建議「鎖 1.0.219」— 兩者皆與原始碼事實不符，已棄用此結論

## Tapedeck 應用（Resilience 原則 #3）

`src/engine/wayland/compositor.rs` 的 Niri/Sway JSON 解析 **不需要任何屬性** — serde 預設忽略上游新增欄位，容錯天然成立。若未來需要「嚴格模式」（未知欄位報錯），才需 `#[serde(deny_unknown_fields)]`（與容錯目標相反，一般不用）。

## 驗證方法（可重現）

```bash
mkdir -p /tmp/serde-repro && cd /tmp/serde-repro
# Cargo.toml: serde = { version = "1.0", features = ["derive"] } + serde_json
# 如上最小重現程式碼
cargo run  # → SUCCESS: 未知欄位被忽略
```
