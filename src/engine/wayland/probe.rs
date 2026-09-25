//! `CompositorError` — compositor 操作的錯誤分類（native-compositor-probe T1／REQ-2）
//!
//! IPC 層（socket 連不上、命令失敗）與查無視窗（業務層）必須可辨識：
//! WaitWindow 輪詢只重試「查無視窗」，IPC 錯誤須立即失敗並保留根因。

use std::fmt;

/// compositor 操作錯誤：IPC 層 vs 查無視窗層（REQ-2）
#[derive(Debug)]
pub enum CompositorError {
    /// IPC 層失敗：io error、非零 exit、JSON 解析失敗。訊息含根因。
    Ipc(String),
    /// IPC 正常、確實查詢了，但沒有符合目標的視窗（可重試）。
    NotFound(String),
}

impl CompositorError {
    /// WaitWindow 輪詢判別：只有查無視窗可重試，IPC 錯誤須立即失敗
    pub fn is_ipc(&self) -> bool {
        matches!(self, Self::Ipc(_))
    }
}

impl fmt::Display for CompositorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ipc(m) => write!(f, "compositor IPC 不可用（{m}）"),
            Self::NotFound(m) => write!(f, "查無視窗：{m}"),
        }
    }
}

impl std::error::Error for CompositorError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_and_ipc_classification() {
        let ipc = CompositorError::Ipc("Connection refused (os error 111)".to_owned());
        assert!(ipc.is_ipc());
        assert!(ipc.to_string().contains("IPC 不可用"));
        assert!(ipc.to_string().contains("os error 111"), "訊息須含根因");

        let nf = CompositorError::NotFound("在 Niri 中找不到符合 'x' 的視窗".to_owned());
        assert!(!nf.is_ipc());
        assert!(nf.to_string().starts_with("查無視窗"));
    }
}
