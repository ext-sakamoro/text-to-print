//! Gallery Phase 3 (2026-08-26) endpoints
//!
//! `GET  /api/gallery/list?limit=N&offset=N`  — latest-first listing
//! `POST /api/gallery/publish`                — insert with ed25519 sig verify
//! `DELETE /api/gallery/{id}` (body: sig + did) — own-post soft delete
//!
//! All storage lives in the `GALLERY_DB` D1 binding (see wrangler.toml
//! and `migrations/0003_gallery.sql`)
//!
//! Rate limits reuse the shared `rate_limit_counters` table on the
//! `SHARES_DB` binding — same per-UUID / per-IP hourly budget as
//! `/api/share` (`PER_UUID_HOURLY` / `PER_IP_HOURLY` env vars)

use serde::{Deserialize, Serialize};
use worker::{Date, Request, Response, Result as WorkerResult, RouteContext, Url};

use crate::rate_limit::{self, LimitDecision};
use crate::validate::{self, GalleryPublishFields};
use crate::{hash_ip, RejectResponse};

const IP_HEADER: &str = "CF-Connecting-IP";
const DEFAULT_IP_SALT: &str = "unsalted";
const DEFAULT_PER_UUID_HOURLY: u32 = 10;
const DEFAULT_PER_IP_HOURLY: u32 = 100;
const DEFAULT_LIST_LIMIT: u32 = 100;
const MAX_LIST_LIMIT: u32 = 500;

/// Wire schema for `POST /api/gallery/publish`
#[derive(Debug, Deserialize)]
pub struct PublishRequest {
    pub id: String,
    pub author_did: String,
    pub author_nickname: Option<String>,
    pub lol_source: String,
    pub created_at: String,
    pub signature: String,
}

/// Wire schema for `DELETE /api/gallery/{id}` request body
#[derive(Debug, Deserialize)]
pub struct DeleteRequest {
    pub author_did: String,
    pub signature: String,
}

/// Wire schema for a single Gallery item in the list response
#[derive(Debug, Serialize, Deserialize)]
pub struct GalleryItem {
    pub id: String,
    pub author_did: String,
    pub author_nickname: Option<String>,
    pub lol_source: String,
    pub created_at: String,
}

/// Wire schema for `GET /api/gallery/list`
#[derive(Debug, Serialize)]
pub struct ListResponse {
    pub items: Vec<GalleryItem>,
    pub next_offset: Option<u32>,
}

/// Wire schema for `POST /api/gallery/publish` success
#[derive(Debug, Serialize)]
pub struct PublishAccept {
    pub status: &'static str,
    pub id: String,
    pub received_at: String,
}

/// Wire schema for `DELETE /api/gallery/{id}` success
#[derive(Debug, Serialize)]
pub struct DeleteAccept {
    pub status: &'static str,
    pub id: String,
}

pub async fn handle_list(req: &Request, ctx: &RouteContext<()>) -> WorkerResult<Response> {
    let url = req.url()?;
    let (limit, offset) = parse_list_query(&url);
    let db = match ctx.env.d1("GALLERY_DB") {
        Ok(d) => d,
        Err(e) => return reject(500, "gallery_db_unbound", &e.to_string()),
    };
    let rows = db
        .prepare(
            "SELECT id, author_did, author_nickname, lol_source, created_at
             FROM gallery_sdfs
             WHERE deleted_at IS NULL
             ORDER BY created_at DESC
             LIMIT ?1 OFFSET ?2",
        )
        .bind(&[limit.into(), offset.into()])?
        .all()
        .await?;
    let items: Vec<GalleryItem> = rows.results::<GalleryItem>()?;
    let next_offset = if items.len() as u32 == limit {
        Some(offset + limit)
    } else {
        None
    };
    Response::from_json(&ListResponse { items, next_offset })
}

pub async fn handle_publish(
    req: &mut Request,
    ctx: &RouteContext<()>,
) -> WorkerResult<Response> {
    let body: PublishRequest = match req.json().await {
        Ok(b) => b,
        Err(e) => return reject(400, "invalid_json", &e.to_string()),
    };

    // Signature verify + format checks (fail-fast validator)
    let fields = GalleryPublishFields {
        id: &body.id,
        author_did: &body.author_did,
        author_nickname: body.author_nickname.as_deref(),
        lol_source: &body.lol_source,
        created_at: &body.created_at,
        signature: &body.signature,
    };
    if let Err(reason) = validate::check_gallery_publish(&fields) {
        return reject(400, reason.tag(), reason.message());
    }

    // Rate limit (shared with /api/share; per-UUID + per-IP hourly bucket)
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

    let shares_db = ctx.env.d1("SHARES_DB")?;
    let uuid_decision = rate_limit::bump(
        &shares_db,
        &format!("gallery_uuid:{}", body.id),
        per_uuid,
        &now_iso,
        &hour_end_iso,
    )
    .await?;
    if uuid_decision == LimitDecision::Deny {
        return reject(429, "rate_limit_uuid", "per-UUID hourly cap exceeded");
    }
    let ip_decision = rate_limit::bump(
        &shares_db,
        &format!("gallery_ip:{ip_hash}"),
        per_ip,
        &now_iso,
        &hour_end_iso,
    )
    .await?;
    if ip_decision == LimitDecision::Deny {
        return reject(429, "rate_limit_ip", "per-IP hourly cap exceeded");
    }

    // Insert (idempotent on id PK — replay writes are absorbed)
    let gallery_db = ctx.env.d1("GALLERY_DB")?;
    gallery_db
        .prepare(
            "INSERT INTO gallery_sdfs (id, author_did, author_nickname, lol_source, created_at, signature)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO NOTHING",
        )
        .bind(&[
            body.id.clone().into(),
            body.author_did.into(),
            body.author_nickname
                .map_or(worker::wasm_bindgen::JsValue::NULL, worker::wasm_bindgen::JsValue::from),
            body.lol_source.into(),
            body.created_at.into(),
            body.signature.into(),
        ])?
        .run()
        .await?;

    let resp = Response::from_json(&PublishAccept {
        status: "accepted",
        id: body.id,
        received_at: now_iso,
    })?;
    Ok(resp.with_status(202))
}

pub async fn handle_delete(
    req: &mut Request,
    ctx: &RouteContext<()>,
) -> WorkerResult<Response> {
    let id = match ctx.param("id") {
        Some(v) => v.clone(),
        None => return reject(400, "missing_id", "path param `id` required"),
    };
    let body: DeleteRequest = match req.json().await {
        Ok(b) => b,
        Err(e) => return reject(400, "invalid_json", &e.to_string()),
    };

    // Verify sig against canonical delete msg (distinct from publish msg
    // so a stolen publish sig cannot be replayed as a delete)
    let canonical = validate::gallery_delete_canonical(&id, &body.author_did);
    if let Err(reason) =
        validate::verify_gallery_signature(&body.author_did, &body.signature, canonical.as_bytes())
    {
        return reject(400, reason.tag(), reason.message());
    }

    // Only allow delete if the author matches the DB row (defence in
    // depth — sig verify already binds author_did, but the DB check
    // catches races where someone forks a row and then tries to nuke
    // the original)
    let db = ctx.env.d1("GALLERY_DB")?;
    let result = db
        .prepare(
            "UPDATE gallery_sdfs SET deleted_at = CURRENT_TIMESTAMP
             WHERE id = ?1 AND author_did = ?2 AND deleted_at IS NULL",
        )
        .bind(&[id.clone().into(), body.author_did.into()])?
        .run()
        .await?;

    // D1 returns changes count in meta — 0 means either not found or
    // already deleted or ownership mismatch All 3 collapse to 404 so
    // no info leak on which case triggered
    let changed = result.meta()?.and_then(|m| m.changes).unwrap_or(0);
    if changed == 0 {
        return reject(404, "gallery_not_found", "no matching row for this author");
    }

    let resp = Response::from_json(&DeleteAccept {
        status: "deleted",
        id,
    })?;
    Ok(resp.with_status(200))
}

fn parse_list_query(url: &Url) -> (u32, u32) {
    let mut limit = DEFAULT_LIST_LIMIT;
    let mut offset: u32 = 0;
    for (k, v) in url.query_pairs() {
        match k.as_ref() {
            "limit" => {
                if let Ok(n) = v.parse::<u32>() {
                    limit = n.clamp(1, MAX_LIST_LIMIT);
                }
            }
            "offset" => {
                if let Ok(n) = v.parse::<u32>() {
                    offset = n;
                }
            }
            _ => {}
        }
    }
    (limit, offset)
}

fn reject(status: u16, reason: &'static str, details: &str) -> WorkerResult<Response> {
    let body = RejectResponse {
        status: "rejected",
        reason,
        details: details.to_string(),
    };
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
