//! tapedeck doctor — 系統依賴檢查（Resilience 原則 2）
//!
//! 結構化 deps 表檢查外部工具：實際執行 `--version`（非 which，
//! 可偵測損壞/權限不足），靜默執行只關心存不存在，逐項輸出
//! ✅ OK / ❌ MISSING + Hint。最後調用硬體探針寫回 `[system.detected]`。

use std::path::Path;
use std::process::Command;

use crate::config;
use crate::engine::input::InputBackend;
use crate::engine::probe;
/// 依賴定義：名稱、版本旗標、用途說明（Hint）
struct Dep {
    name: &'static str,
    version_flag: &'static str,
    hint: &'static str,
}

const DEPS: &[Dep] = &[
    Dep {
        name: "vhs",
        version_flag: "--version",
        hint: "TUI 錄製與編排所需（.roll → .tape 轉譯後驅動）",
    },
    Dep {
        name: "ffmpeg",
        version_flag: "-version",
        hint: "影片編碼與處理所需（編碼器掃描、媒體優化）",
    },
    Dep {
        name: "wf-recorder",
        version_flag: "-v",
        hint: "Wayland 螢幕錄製所需（GUI 軌錄製）",
    },
];

/// 執行 tapedeck doctor：檢查依賴 + 硬體探針寫回 config
pub fn run_doctor() {
    println!("🩺 Checking system dependencies for tapedeck.\n");

    let mut all_ok = true;
    for dep in DEPS {
        if is_available(dep) {
            println!("✅ [OK] {} found.", dep.name);
        } else {
            println!("❌ [MISSING] {} not found.", dep.name);
            println!("   └─ Hint: {}", dep.hint);
            all_ok = false;
        }
    }

    // 輸入注入後端診斷（OQ-02 / T10：uinput 優先，wtype 回退）
    if !check_input_provider() {
        all_ok = false;
    }

    // 硬體探針寫回 [system.detected]（probe 為 doctor 的 CLI 消費端）
    let caps = probe::HardwareCapabilities::probe_system();    println!(
        "\n🔍 Hardware: dri={} encoders={}",
        caps.dri,
        caps.encoders.len()
    );
    let detected = config::Detected {
        encoders: caps.encoders.clone(),
        vaapi: caps.vaapi,
        dri: caps.dri,
    };
    let system = config::System {
        detected: Some(detected),
    };
    if let Err(e) = config::save(&system) {
        println!("   └─ ⚠️ 寫回 [system.detected] 失敗: {e}");
        all_ok = false;
    }

    if all_ok {
        println!("\n✨ All systems go! You're ready to tape.");
    } else {
        println!("\n⚠️ Some dependencies are missing. Install them to proceed.");
    }
}

/// 實際執行 `<tool> <version_flag>`，靜默執行，只關心存不存在
fn is_available(dep: &Dep) -> bool {
    Command::new(dep.name)
        .arg(dep.version_flag)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// 輸入注入後端診斷（OQ-02 / T10）：uinput 優先，wtype 回退
///
/// 檢查三項：/dev/uinput 存在、uinput kernel module 已載入、目前使用者
/// 對 /dev/uinput 有寫權限。權限不足時提示修正方式（usermod / udev rule）。
fn check_input_provider() -> bool {
    println!("\n🎮 Input Provider Diagnostic:");

    let dev_ok = Path::new("/dev/uinput").exists();
    println!(
        "{} Device: /dev/uinput {}",
        ok_or_missing(dev_ok),
        if dev_ok { "exists" } else { "missing" }
    );

    let module_ok = uinput_module_loaded();
    println!(
        "{} Kernel Module: uinput {}",
        ok_or_missing(module_ok),
        if module_ok { "loaded" } else { "not loaded" }
    );

    let perm_ok = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/uinput")
        .is_ok();
    println!(
        "{} Permission: Current user {} write access to /dev/uinput",
        ok_or_missing(perm_ok),
        if perm_ok { "has" } else { "has no" }
    );

    let backend = InputBackend::detect();
    println!("👉 Active Input Backend: {backend}");

    if perm_ok {
        true
    } else {
        println!("💡 To unlock full mouse/keyboard automation, fix permission:");
        println!("   Option A: sudo usermod -aG input $USER (and relogin)");
        println!("   Option B: Add udev rule /etc/udev/rules.d/99-input.rules");
        println!("             (KERNEL==\"uinput\", MODE=\"0660\", GROUP=\"input\")");
        false
    }
}

/// uinput kernel module 是否已載入（/sys/class/misc/uinput 存在即載入）
fn uinput_module_loaded() -> bool {
    Path::new("/sys/class/misc/uinput").exists()
}

/// 檢查結果的 ✅/❌ 前綴
fn ok_or_missing(ok: bool) -> &'static str {
    if ok { "✅" } else { "❌" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deps_table_is_well_formed() {
        // 名稱唯一、hint 非空、版本旗標為 --version/-v 形式
        let mut names = std::collections::HashSet::new();
        for dep in DEPS {
            assert!(!dep.hint.is_empty());
            assert!(dep.version_flag.starts_with('-'));
            assert!(names.insert(dep.name), "duplicate dep: {}", dep.name);
        }
        assert_eq!(DEPS.len(), 3, "deps 表擴充時同步更新測試");
    }

    #[test]
    fn is_available_reports_missing_tool() {
        // 不存在的工具 → false（不實際執行外部工具）
        let missing = Dep {
            name: "definitely-not-a-real-tool-xyz",
            version_flag: "--version",
            hint: "test",
        };
        assert!(!is_available(&missing));
    }

    #[test]
    fn is_available_finds_existing_tool() {
        // POSIX 保證存在的工具 → true
        let present = Dep {
            name: "true",
            version_flag: "--version",
            hint: "test",
        };
        assert!(is_available(&present));
    }

    #[test]
    fn ok_or_missing_maps_boolean() {
        assert_eq!(ok_or_missing(true), "✅");
        assert_eq!(ok_or_missing(false), "❌");
    }
}
