//! text-to-print LoRA share upload backend (Cloudflare Workers, Rust wasm32)
//!
//! Endpoints:
//! - `POST /api/share`  — accept a [`SharePayload`] matching v1 alice_manifest schema
//! - `GET  /health`     — liveness probe
//!
//! Wired to D1 (SQLite serverless) via the `SHARES_DB` binding. See
//! `wrangler.toml` and `migrations/0001_init.sql`.
//!
//! ## Privacy
//!
//! Raw client IP is never stored; a `SHA256(ip + IP_HASH_SALT)` hash is
//! recorded only for abuse tracking + rate-limit keys. Salt lives in
//! `wrangler secret put IP_HASH_SALT` (production) or `[vars]` (dev).
//!
//! ## Rate limit
//!
//! Per-UUID (`PER_UUID_HOURLY`) and per-IP-hash (`PER_IP_HOURLY`) rolling
//! hourly counters, tracked in the `rate_limit_counters` D1 table. Cloudflare
//! free tier's 100k req/day global cap is the outermost guard.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use worker::{
    event, Context, Env, Request, Response, Result as WorkerResult, RouteContext, Router,
};

mod rate_limit;
mod share_handler;
mod validate;

/// Wire schema for a LoRA share upload (mirrors
/// `text_to_print_network::share::SharePayload`)
///
/// Kept manually in sync with the client — divergences are caught at
/// validation time and rejected as `schema_invalid`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharePayload {
    pub schema_version: String,
    pub uuid: String,
    pub prompt: String,
    pub prompt_lang: String,
    pub llm_model: String,
    pub lol_source: String,
    pub lol_sha256: String,
    pub mesh_sha256: String,
    pub quality: QualitySignals,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualitySignals {
    pub success: bool,
    pub retry_count: u32,
    pub time_to_file_ms: u64,
    pub safety_violations: Vec<String>,
    pub export_format: String,
    pub user_kept: bool,
    pub user_edited: bool,
}

/// Successful accept response returned to the client
#[derive(Debug, Clone, Serialize)]
pub struct AcceptResponse {
    pub status: &'static str, // "accepted"
    pub receipt_id: String,
    pub timestamp: String,
}

/// Rejection response — reason is a small tag for the client, details are
/// human-readable and safe to log
#[derive(Debug, Clone, Serialize)]
pub struct RejectResponse {
    pub status: &'static str, // "rejected"
    pub reason: &'static str,
    pub details: String,
}

/// Compute the ip-hash key: `SHA256(ip || salt)` hex-encoded
///
/// Salt comes from `IP_HASH_SALT` env var; if missing (misconfiguration) we
/// fall back to `"unsalted"` so the worker still functions but the operator
/// gets a clearly wrong-looking bucket in D1 for follow-up.
#[must_use]
pub fn hash_ip(ip: &str, salt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(ip.as_bytes());
    hasher.update(b"|");
    hasher.update(salt.as_bytes());
    hex::encode(hasher.finalize())
}

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> WorkerResult<Response> {
    let router = Router::new();
    router
        .get("/health", |_, _| Response::ok("ok"))
        .post_async("/api/share", handle_share)
        .run(req, env)
        .await
}

async fn handle_share(mut req: Request, ctx: RouteContext<()>) -> WorkerResult<Response> {
    share_handler::handle(&mut req, &ctx).await
}
