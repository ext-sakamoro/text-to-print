//! `POST /api/share` handler
//!
//! Flow: parse JSON → validate → rate-limit (uuid + ip hash) → insert into D1

use worker::{Date, Request, Response, Result as WorkerResult, RouteContext};

use crate::rate_limit::{self, LimitDecision};
use crate::validate::{self, RejectReason};
use crate::{hash_ip, AcceptResponse, RejectResponse, SharePayload};

const IP_HEADER: &str = "CF-Connecting-IP";
const DEFAULT_IP_SALT: &str = "unsalted";
const DEFAULT_PER_UUID_HOURLY: u32 = 10;
const DEFAULT_PER_IP_HOURLY: u32 = 100;

pub async fn handle(req: &mut Request, ctx: &RouteContext<()>) -> WorkerResult<Response> {
    // 1. Parse
    let payload: SharePayload = match req.json().await {
        Ok(p) => p,
        Err(e) => {
            return reject(400, "invalid_json", &e.to_string());
        }
    };

    // 2. Validate against v1 alice_manifest.schema.json contract
    if let Err(reason) = validate::check(&payload) {
        return reject(400, reason.tag(), reason.message());
    }

    // 3. Rate limit — resolve keys + tunables
    let per_uuid = env_u32(&ctx.env, "PER_UUID_HOURLY", DEFAULT_PER_UUID_HOURLY);
    let per_ip = env_u32(&ctx.env, "PER_IP_HOURLY", DEFAULT_PER_IP_HOURLY);
    let salt = env_str(&ctx.env, "IP_HASH_SALT", DEFAULT_IP_SALT);
    let ip = req
        .headers()
        .get(IP_HEADER)
        .ok()
        .flatten()
        .unwrap_or_else(|| "unknown".to_string());
    let ip_hash = hash_ip(&ip, &salt);

    let now = Date::now();
    let now_iso = now.to_string();
    let hour_end_iso = {
        let ms = now.as_millis();
        let ms_per_hour: u64 = 60 * 60 * 1000;
        let end_ms = ms - (ms % ms_per_hour) + ms_per_hour;
        Date::new(worker::DateInit::Millis(end_ms)).to_string()
    };

    let db = ctx.env.d1("SHARES_DB")?;

    let uuid_decision = rate_limit::bump(
        &db,
        &format!("uuid:{}", payload.uuid),
        per_uuid,
        &now_iso,
        &hour_end_iso,
    )
    .await?;
    if uuid_decision == LimitDecision::Deny {
        return reject(429, "rate_limit_uuid", "per-UUID hourly cap exceeded");
    }

    let ip_decision = rate_limit::bump(
        &db,
        &format!("ip:{ip_hash}"),
        per_ip,
        &now_iso,
        &hour_end_iso,
    )
    .await?;
    if ip_decision == LimitDecision::Deny {
        return reject(429, "rate_limit_ip", "per-IP hourly cap exceeded");
    }

    // 4. Insert into D1 (idempotent on uuid PK — replay writes are absorbed)
    let violations_json = serde_json::to_string(&payload.quality.safety_violations)
        .map_err(|e| worker::Error::RustError(e.to_string()))?;

    db.prepare(
        "INSERT INTO shares (
            uuid, timestamp, prompt, prompt_lang, llm_model,
            lol_source, lol_sha256, mesh_sha256,
            success, retry_count, time_to_file_ms, safety_violations_json, export_format,
            user_kept, user_edited,
            ip_hash
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
        ON CONFLICT(uuid) DO NOTHING",
    )
    .bind(&[
        payload.uuid.clone().into(),
        now_iso.clone().into(),
        payload.prompt.into(),
        payload.prompt_lang.into(),
        payload.llm_model.into(),
        payload.lol_source.into(),
        payload.lol_sha256.into(),
        payload.mesh_sha256.into(),
        i32::from(payload.quality.success).into(),
        payload.quality.retry_count.into(),
        payload.quality.time_to_file_ms.into(),
        violations_json.into(),
        payload.quality.export_format.into(),
        i32::from(payload.quality.user_kept).into(),
        i32::from(payload.quality.user_edited).into(),
        ip_hash.into(),
    ])?
    .run()
    .await?;

    let body = AcceptResponse {
        status: "accepted",
        receipt_id: payload.uuid,
        timestamp: now_iso,
    };
    let resp = Response::from_json(&body)?;
    Ok(resp.with_status(202))
}

fn reject(status: u16, reason: &'static str, details: &str) -> WorkerResult<Response> {
    let body = RejectResponse {
        status: "rejected",
        reason,
        details: details.to_string(),
    };
    // schema-level reasons all round to 400; rate limits are 429
    let http = if matches!(reason, "rate_limit_uuid" | "rate_limit_ip") {
        429
    } else {
        status
    };
    Response::from_json(&body).map(|r| r.with_status(http))
}

fn env_str(env: &worker::Env, key: &str, default: &str) -> String {
    env.var(key)
        .ok()
        .map(|v| v.to_string())
        .unwrap_or_else(|| default.to_string())
}

fn env_u32(env: &worker::Env, key: &str, default: u32) -> u32 {
    env.var(key)
        .ok()
        .and_then(|v| v.to_string().parse::<u32>().ok())
        .unwrap_or(default)
}

/// Reasons the handler surfaces are re-exported so client-side handling can
/// pattern-match on the same string keys
#[allow(dead_code)]
pub const REJECT_REASONS: &[(&str, u16)] = &[
    ("invalid_json", 400),
    ("rate_limit_uuid", 429),
    ("rate_limit_ip", 429),
    // schema tags are enumerated in `RejectReason::tag`
];

#[allow(dead_code)]
fn schema_reject_tags() -> [&'static str; 11] {
    [
        RejectReason::SchemaVersionMismatch.tag(),
        RejectReason::UuidFormat.tag(),
        RejectReason::PromptEmpty.tag(),
        RejectReason::PromptTooLong.tag(),
        RejectReason::LolSourceEmpty.tag(),
        RejectReason::LolSourceTooLong.tag(),
        RejectReason::LolSha256Format.tag(),
        RejectReason::MeshSha256Format.tag(),
        RejectReason::ExportFormatUnknown.tag(),
        RejectReason::LlmModelEmpty.tag(),
        RejectReason::PromptLangFormat.tag(),
    ]
}
