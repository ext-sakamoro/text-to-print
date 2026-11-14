//! 同梱の既定 preset の正本 (1 つだけ)
//!
//! file は `crates/worker/src/default_presets.json` (配信 API の fallback と同じ file) app はこの定数を、
//! 配信 API と network の試験は同じ file を include する (`tests/presets_single_source.rs` が確かめる)
//! core は git 依存で repo ごと取り出されるので、iOS の bridge からもこの定数で読める

/// 既定 preset (JSON)
pub const DEFAULT_PRESETS_JSON: &str = include_str!("../../worker/src/default_presets.json");
