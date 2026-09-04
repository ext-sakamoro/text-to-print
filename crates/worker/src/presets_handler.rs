//! `GET /api/presets` — text-to-print archetype preset library 配信 (Sprint X.1)
//!
//! 3-layer archetype library の Layer 1 (preset) を Cloudflare KV 経由で
//! 全 user に startup sync させるための endpoint 詳細:
//! `memory/project_text_to_print_archetype_library_architecture.md`
//!
//! ## 動作
//!
//! 1. KV `PRESETS_KV` binding から `PRESETS_V1` key を fetch
//! 2. KV 未設定 or empty なら bundled `default_presets.json` を fallback として返却
//!    (β 初期 seed data、これで KV 未 populate 状態でも app が動く)
//! 3. Response に `ETag: "<version>"` header 付与、client 側 `If-None-Match` 対応
//!
//! ## Privacy
//!
//! 認証なし public endpoint、rate limit なし (単純 KV read で cost 極小)
//! 将来的な rate limit 追加は Cloudflare 側 firewall rule で対応可
//!
//! ## Update workflow (user 側、post-deploy)
//!
//! ```bash
//! # local で default_presets.json 編集後
//! wrangler kv:key put --binding=PRESETS_KV PRESETS_V1 --path src/default_presets.json
//! # → 全 user が次回起動時に新 preset を fetch
//! ```

use worker::{Request, Response, Result as WorkerResult, RouteContext};

/// Bundled seed data、KV 未 populate 時の fallback
///
/// β 公開初期は KV に投入していない可能性、または network 経路失敗時にも
/// worker 側で最低限の preset を返せるように compile-time embed
const DEFAULT_PRESETS_JSON: &str = include_str!("default_presets.json");

/// KV binding name (wrangler.toml `[[kv_namespaces]] binding = "PRESETS_KV"`)
const KV_BINDING: &str = "PRESETS_KV";

/// KV key で preset JSON 全体を保存する (単一 blob 方式、update = replace)
const KV_KEY: &str = "PRESETS_V1";

/// `GET /api/presets` handler
pub async fn handle(req: &Request, ctx: &RouteContext<()>) -> WorkerResult<Response> {
    // KV から fetch 試みる、失敗 or 空なら bundled fallback
    let (body, source) = match ctx.kv(KV_BINDING) {
        Ok(kv) => match kv.get(KV_KEY).text().await {
            Ok(Some(text)) if !text.trim().is_empty() => (text, "kv"),
            _ => (DEFAULT_PRESETS_JSON.to_string(), "default"),
        },
        Err(_) => (DEFAULT_PRESETS_JSON.to_string(), "default"),
    };

    // Extract version from JSON (best-effort) for ETag
    let etag = extract_version(&body).unwrap_or_else(|| "unknown".to_string());
    let quoted_etag = format!("\"{etag}\"");

    // 304 optimization: client 側 If-None-Match 一致なら body 省略
    if let Ok(headers) = req.headers().get("If-None-Match") {
        if let Some(client_etag) = headers {
            if client_etag == quoted_etag {
                let mut resp = Response::empty()?.with_status(304);
                let hdrs = resp.headers_mut();
                let _ = hdrs.set("ETag", &quoted_etag);
                let _ = hdrs.set("X-Presets-Source", source);
                return Ok(resp);
            }
        }
    }

    let mut resp = Response::ok(body)?;
    let hdrs = resp.headers_mut();
    let _ = hdrs.set("Content-Type", "application/json");
    let _ = hdrs.set("ETag", &quoted_etag);
    let _ = hdrs.set("X-Presets-Source", source);
    let _ = hdrs.set(
        "Cache-Control",
        "public, max-age=300, stale-while-revalidate=86400",
    );
    Ok(resp)
}

/// JSON 先頭 200 chars 内から `"version": "..."` を naive 抽出 (JSON parse 回避で軽量)
fn extract_version(json: &str) -> Option<String> {
    let head: String = json.chars().take(200).collect();
    let key_pos = head.find("\"version\"")?;
    let after_key = &head[key_pos + 9..];
    let colon_pos = after_key.find(':')?;
    let after_colon = &after_key[colon_pos + 1..];
    let quote_start = after_colon.find('"')?;
    let value_start = &after_colon[quote_start + 1..];
    let quote_end = value_start.find('"')?;
    Some(value_start[..quote_end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_presets_json_parseable() {
        // bundled seed が正しい JSON で、version + categories を持つか
        let v: serde_json::Value = serde_json::from_str(DEFAULT_PRESETS_JSON)
            .expect("default_presets.json is valid JSON");
        assert!(v.get("version").is_some(), "must have version");
        assert!(
            v.get("categories").and_then(|c| c.as_array()).is_some(),
            "must have categories array"
        );
    }

    #[test]
    fn default_presets_has_seed_data() {
        // 累積 25 categories / 161 preset (Sprint 21-22 + Multi-domain 4 domain 展開後、2026-09-04)
        // categories.len() は cross-crate assertion pattern (presets_client と同型)
        let v: serde_json::Value = serde_json::from_str(DEFAULT_PRESETS_JSON).unwrap();
        let cats = v["categories"].as_array().unwrap();
        assert_eq!(cats.len(), 25, "25 categories (実績品 2 + 生活雑貨系 17 + 機械要素系 4 + 家具 + 建築 + 電子工作)");
        let total_presets: usize = cats
            .iter()
            .map(|c| c["presets"].as_array().map(Vec::len).unwrap_or(0))
            .sum();
        assert_eq!(
            total_presets, 161,
            "161 preset total (Sprint 21-22 + Multi-domain 4 domain 展開)"
        );
    }

    #[test]
    fn extract_version_finds_version_string() {
        let sample = r#"{"version":"2026-08-20T00:00:00Z","categories":[]}"#;
        assert_eq!(
            extract_version(sample).as_deref(),
            Some("2026-08-20T00:00:00Z")
        );
    }

    #[test]
    fn extract_version_handles_whitespace() {
        let sample = r#"{
  "version"  :  "v1.0.0" ,
  "categories": []
}"#;
        assert_eq!(extract_version(sample).as_deref(), Some("v1.0.0"));
    }

    #[test]
    fn extract_version_returns_none_when_missing() {
        assert_eq!(extract_version("{}"), None);
    }
}
