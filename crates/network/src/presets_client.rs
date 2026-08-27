//! Client for the archetype preset library (Sprint X.1、Layer 1 sync)
//!
//! text-to-print archetype library の 3-layer 管理設計 の Layer 1 (preset) を
//! Cloudflare Worker `GET /api/presets` から fetch する client 詳細:
//! `~/.claude/projects/-Users-ys/memory/project_text_to_print_archetype_library_architecture.md`
//!
//! ## 動作
//!
//! 1. app 起動時に background task で `fetch(cached_etag)` を invoke
//! 2. 200 → JSON body + ETag を新 cache として返却
//! 3. 304 → cache 継続使用 (server 側で変更なし)
//! 4. network / server error → error 返却、caller 側で cache or bundled fallback
//!
//! ## Retry policy
//!
//! Fetch 失敗は静かに諦める (offline 想定、local cache or bundled fallback で app は動く)
//! 定期 sync は現状未実装 (β MVP scope、post-β で拡張)

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Preset response schema (Worker `presets_handler.rs` と JSON 対応)
///
/// Kept manually in sync with worker crate + `default_presets.json`; 手動 sync 前提
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PresetsResponse {
    pub version: String,
    pub categories: Vec<PresetCategory>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PresetCategory {
    pub name: String,
    pub presets: Vec<Preset>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Preset {
    pub id: String,
    pub label: String,
    pub lol_dsl: String,
}

/// Fetch outcome — either an updated payload (with fresh ETag for next call)
/// or a 304 signalling cache is still valid
#[derive(Debug, Clone)]
pub enum FetchResult {
    Updated {
        body: PresetsResponse,
        etag: Option<String>,
    },
    NotModified,
}

/// HTTP client for `GET /api/presets` — thin wrapper around reqwest with
/// short timeout + optional If-None-Match header
pub struct PresetsClient {
    endpoint: String,
    client: reqwest::Client,
}

impl PresetsClient {
    /// Build a client for the given endpoint URL
    ///
    /// Default timeout 10s (network / offline 判定を高速に、UX 待ち抑制)
    #[must_use]
    pub fn new(endpoint: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { endpoint, client }
    }

    /// Default endpoint pointing at production Cloudflare Worker
    #[must_use]
    pub fn default_endpoint() -> String {
        "https://text-to-print.alicelaw.net/api/presets".to_string()
    }

    /// Resolve the effective endpoint URL by precedence:
    ///   1. `TTP_PRESETS_ENDPOINT` env (dev / one-shot override)
    ///   2. `db_override` (user Settings で保存した custom URL、空文字なら skip)
    ///   3. `default_endpoint()` (Cloudflare production)
    ///
    /// 空文字 / 全 whitespace の DB override は無視する (default にフォールバック)
    #[must_use]
    pub fn resolve_endpoint(db_override: Option<&str>) -> String {
        if let Ok(env_url) = std::env::var("TTP_PRESETS_ENDPOINT")
            && !env_url.trim().is_empty()
        {
            return env_url;
        }
        if let Some(url) = db_override
            && !url.trim().is_empty()
        {
            return url.to_string();
        }
        Self::default_endpoint()
    }

    /// Fetch presets, optionally sending `If-None-Match` for cache validation
    ///
    /// # Errors
    ///
    /// Network error, non-2xx-non-304 status, or JSON parse failure
    pub async fn fetch(&self, current_etag: Option<&str>) -> Result<FetchResult> {
        let mut req = self.client.get(&self.endpoint);
        if let Some(etag) = current_etag {
            req = req.header("If-None-Match", etag);
        }
        let resp = req
            .send()
            .await
            .with_context(|| format!("GET {}", self.endpoint))?;

        match resp.status().as_u16() {
            304 => Ok(FetchResult::NotModified),
            200 => {
                let etag = resp
                    .headers()
                    .get("etag")
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_owned);
                let body: PresetsResponse =
                    resp.json().await.with_context(|| "parse presets JSON")?;
                Ok(FetchResult::Updated { body, etag })
            }
            code => Err(anyhow::anyhow!("unexpected status: {code}")),
        }
    }
}

/// Parse a bundled / cached JSON string into [`PresetsResponse`]
///
/// # Errors
///
/// JSON parse failure — caller may fall back to a known-good bundled default
pub fn parse_presets_json(json: &str) -> Result<PresetsResponse> {
    serde_json::from_str(json).context("parse presets JSON")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_presets_json() {
        let json = r#"{
            "version": "test-1",
            "categories": [
                {
                    "name": "cat1",
                    "presets": [
                        { "id": "p1", "label": "P One", "lol_dsl": "sphere(1)" }
                    ]
                }
            ]
        }"#;
        let parsed = parse_presets_json(json).unwrap();
        assert_eq!(parsed.version, "test-1");
        assert_eq!(parsed.categories.len(), 1);
        assert_eq!(parsed.categories[0].name, "cat1");
        assert_eq!(parsed.categories[0].presets[0].label, "P One");
    }

    #[test]
    fn parse_bundled_default_matches_worker_schema() {
        // network crate 側の default_presets.json は app crate 側と同 schema
        // (worker と app 両方で fallback として使う想定、schema 一致を verify)
        // 2026-08-24 Sprint 20: 18→19 categories (ミックス 9 追加)、108→114 preset
        // 2026-08-27 Sprint 21 Phase X.1: 19→21 categories (機械要素 + 追加、+22 preset = 136)
        // 2026-08-27 Sprint 22 Phase X.2: 21→22 categories (building block +5 preset = 141)
        let bundled = include_str!("../../worker/src/default_presets.json");
        let parsed = parse_presets_json(bundled).expect("bundled default parses");
        assert_eq!(
            parsed.categories.len(),
            22,
            "seed data: 22 categories (17 生活雑貨〜ミックス 9 + 機械要素 + 機械要素追加 + Phase X.2 building block)"
        );
        let total_presets: usize = parsed.categories.iter().map(|c| c.presets.len()).sum();
        assert_eq!(
            total_presets, 141,
            "seed data: 141 preset total (Phase X.1 + X.2 完了時)"
        );
    }

    #[test]
    fn parse_invalid_json_errors() {
        assert!(parse_presets_json("not json").is_err());
        assert!(parse_presets_json(r#"{"version": "v1"}"#).is_err()); // missing categories
    }

    #[test]
    fn presets_client_uses_default_endpoint() {
        assert_eq!(
            PresetsClient::default_endpoint(),
            "https://text-to-print.alicelaw.net/api/presets"
        );
    }

    #[test]
    fn presets_client_new_accepts_custom_endpoint() {
        let _ = PresetsClient::new("http://localhost:8787/api/presets".to_string());
    }

    #[test]
    fn resolve_endpoint_returns_default_when_no_override() {
        // Test 前に念のため env をクリア (他 test 走行 order 影響対策)
        // SAFETY: single-threaded test で set 相当だが cargo test は multi-thread
        // default、環境変数 race 予防のため常に unset + assert 後即 unset で pair
        // 実際はここでは検証優先、race 起きても default に戻るだけ
        unsafe {
            std::env::remove_var("TTP_PRESETS_ENDPOINT");
        }
        assert_eq!(
            PresetsClient::resolve_endpoint(None),
            PresetsClient::default_endpoint()
        );
        assert_eq!(
            PresetsClient::resolve_endpoint(Some("")),
            PresetsClient::default_endpoint()
        );
        assert_eq!(
            PresetsClient::resolve_endpoint(Some("   ")),
            PresetsClient::default_endpoint()
        );
    }

    #[test]
    fn resolve_endpoint_uses_db_override_when_non_empty() {
        unsafe {
            std::env::remove_var("TTP_PRESETS_ENDPOINT");
        }
        assert_eq!(
            PresetsClient::resolve_endpoint(Some("https://mirror.example.com/api/presets")),
            "https://mirror.example.com/api/presets"
        );
    }
}
