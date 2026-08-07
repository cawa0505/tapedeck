//! 媒體匯出（P1 media-export / Filmstrip）
//!
//! 公開 API：`optimize()`（T2）、`filmstrip()`（T4）。
//! ffmpeg 以 CLI 子程序呼叫（適配器模式，Resilience 原則 1），不新增 crate 依賴。

pub mod ffmpeg;
pub mod optimize;
pub mod timeline;
