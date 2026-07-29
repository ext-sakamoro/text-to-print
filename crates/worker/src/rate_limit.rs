//! Rolling hourly rate limit backed by D1
//!
//! Two keys per request: `uuid:{uuid}` and `ip:{ip_hash}` Each has an
//! independent hourly bucket that resets at the top of the next UTC hour
//!
//! The tuning is intentionally generous for a solo desktop app; bump the
//! `PER_UUID_HOURLY` / `PER_IP_HOURLY` env vars in wrangler.toml or via
//! `wrangler secret put` after real traffic data lands

use worker::{D1Database, Result as WorkerResult};

/// Outcome of a bump attempt
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitDecision {
    /// Under the cap after this bump — proceed
    Allow,
    /// Cap already reached (or would be exceeded) — reject with 429
    Deny,
}

/// Increment the counter for `key` and return whether the caller is under
/// the hourly `limit` The `now_iso` / `hour_end_iso` are passed in so tests
/// don't need to depend on the Workers time source
///
/// # Errors
///
/// - Any D1 error from the underlying `prepare` / `run`
pub async fn bump(
    db: &D1Database,
    key: &str,
    limit: u32,
    now_iso: &str,
    hour_end_iso: &str,
) -> WorkerResult<LimitDecision> {
    // 1. Fetch current counter row if the window hasn't expired
    let row = db
        .prepare(
            "SELECT count, window_ends FROM rate_limit_counters WHERE key = ?1 AND window_ends > ?2",
        )
        .bind(&[key.into(), now_iso.into()])?
        .first::<Row>(None)
        .await?;

    let new_count = row.as_ref().map_or(1, |r| r.count + 1);
    let over_limit = new_count > limit;

    if over_limit {
        // Don't record the over-limit hit; the existing counter is already at
        // (or above) the cap
        return Ok(LimitDecision::Deny);
    }

    // Upsert. On conflict (row exists), we set count = new_count so concurrent
    // bumps race-safely converge to the higher value
    db.prepare(
        "INSERT INTO rate_limit_counters (key, count, window_ends) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET count = ?2, window_ends = ?3",
    )
    .bind(&[key.into(), new_count.into(), hour_end_iso.into()])?
    .run()
    .await?;

    Ok(LimitDecision::Allow)
}

#[derive(serde::Deserialize)]
struct Row {
    count: u32,
    #[allow(dead_code)]
    window_ends: String,
}
