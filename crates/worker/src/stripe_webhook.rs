//! Stripe webhook receiver (Phase S1)
//!
//! Verifies `Stripe-Signature` header, dedupes by event id, and dispatches
//! to the appropriate handler for the subset of events we care about:
//!
//! - `checkout.session.completed` — first successful payment, mint license
//! - `customer.subscription.updated` — plan / status change, re-mint license
//!   with fresh expiry
//! - `customer.subscription.deleted` — cancellation, revoke license
//!
//! All other events are acknowledged 200 OK so Stripe stops retrying and we
//! keep the dashboard clean

use chrono::{DateTime, TimeZone, Utc};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;
use subtle::ConstantTimeEq;
use worker::{console_log, D1Database, Env, Request, Response, Result as WorkerResult, RouteContext};

use crate::email;
use crate::license_issue::LicenseIssuer;

type HmacSha256 = Hmac<Sha256>;

/// Stripe signs webhooks with `t=<unix>,v1=<hex(hmac_sha256(t + . + body))>`
/// The comma-separated header may contain multiple `v1=` entries during
/// signing secret rotation — we accept the payload if any of them matches
///
/// See https://stripe.com/docs/webhooks/signatures#verify-manually
pub fn verify_signature(
    payload: &str,
    header: &str,
    secret: &str,
    tolerance_seconds: i64,
    now_unix: i64,
) -> Result<(), WebhookError> {
    let mut ts: Option<i64> = None;
    let mut sigs: Vec<Vec<u8>> = Vec::new();

    for part in header.split(',') {
        let (k, v) = part.split_once('=').ok_or(WebhookError::MalformedSignature)?;
        match k {
            "t" => {
                ts = Some(
                    v.parse::<i64>()
                        .map_err(|_| WebhookError::MalformedSignature)?,
                );
            }
            "v1" => {
                let bytes = hex::decode(v).map_err(|_| WebhookError::MalformedSignature)?;
                sigs.push(bytes);
            }
            _ => {} // ignore v0 (test-mode only) and unknown schemes
        }
    }

    let ts = ts.ok_or(WebhookError::MalformedSignature)?;
    if (now_unix - ts).abs() > tolerance_seconds {
        return Err(WebhookError::TimestampOutOfTolerance);
    }
    if sigs.is_empty() {
        return Err(WebhookError::NoV1Signature);
    }

    let signed_payload = format!("{ts}.{payload}");
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| WebhookError::InvalidSecret)?;
    mac.update(signed_payload.as_bytes());
    let expected = mac.finalize().into_bytes();

    for sig in &sigs {
        if sig.len() == expected.len() && sig.ct_eq(&expected).unwrap_u8() == 1 {
            return Ok(());
        }
    }
    Err(WebhookError::SignatureMismatch)
}

#[derive(Debug, PartialEq, Eq)]
pub enum WebhookError {
    MalformedSignature,
    TimestampOutOfTolerance,
    NoV1Signature,
    InvalidSecret,
    SignatureMismatch,
}

impl std::fmt::Display for WebhookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MalformedSignature => write!(f, "malformed Stripe-Signature header"),
            Self::TimestampOutOfTolerance => write!(f, "webhook timestamp outside tolerance window"),
            Self::NoV1Signature => write!(f, "no v1 signature entries in header"),
            Self::InvalidSecret => write!(f, "signing secret rejected by HMAC init"),
            Self::SignatureMismatch => write!(f, "no v1 signature matched computed HMAC"),
        }
    }
}

// ────────────────────────────────────────────────────────
// Event dispatch
// ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct StripeEvent {
    pub id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub data: StripeEventData,
}

#[derive(Debug, Deserialize)]
pub struct StripeEventData {
    pub object: serde_json::Value,
}

/// Map Stripe price id → tier granted (env-configured so ops can add
/// new price ids without redeploying)
///
/// Currently:
///   - `PRICE_ID_PRO_MONTHLY` / `PRICE_ID_PRO_YEARLY` → `"Pro"`
///   - Enterprise is handled outside Stripe (manual invoicing) so has no
///     price id mapping
pub fn tier_for_price_id(env: &Env, price_id: &str) -> Option<String> {
    for (var, tier) in [
        ("PRICE_ID_PRO_MONTHLY", "Pro"),
        ("PRICE_ID_PRO_YEARLY", "Pro"),
    ] {
        if let Ok(val) = env.var(var) {
            if val.to_string() == price_id {
                return Some(tier.to_string());
            }
        }
        // Also check secrets (wrangler secret put lives in `env.secret`)
        if let Ok(val) = env.secret(var) {
            if val.to_string() == price_id {
                return Some(tier.to_string());
            }
        }
    }
    None
}

pub async fn handle(req: &mut Request, ctx: &RouteContext<()>) -> WorkerResult<Response> {
    // 1. Read raw body — signature is computed over the exact bytes so
    //    we must not re-serialize the JSON
    let payload = req.text().await?;

    // 2. Verify signature
    let sig_header = req
        .headers()
        .get("Stripe-Signature")?
        .ok_or_else(|| worker::Error::from("missing Stripe-Signature header"))?;
    let secret = ctx
        .env
        .secret("STRIPE_WEBHOOK_SECRET")
        .map_err(|_| worker::Error::from("STRIPE_WEBHOOK_SECRET not configured"))?
        .to_string();

    let now_unix = (worker::Date::now().as_millis() / 1000) as i64;
    if let Err(e) = verify_signature(&payload, &sig_header, &secret, 300, now_unix) {
        console_log!("stripe webhook signature verify failed: {e}");
        return Response::error(format!("signature verify: {e}"), 400);
    }

    // 3. Parse event
    let event: StripeEvent = serde_json::from_str(&payload)
        .map_err(|e| worker::Error::from(format!("invalid stripe event json: {e}")))?;

    // 4. Idempotency: dedupe by event id (Stripe delivers at-least-once)
    let db = ctx.env.d1("SHARES_DB")?;
    let dupe = db
        .prepare("SELECT stripe_event_id FROM webhook_events WHERE stripe_event_id = ?")
        .bind(&[event.id.clone().into()])?
        .first::<serde_json::Value>(None)
        .await?;
    if dupe.is_some() {
        console_log!("stripe webhook duplicate event {}, acknowledging", event.id);
        return Response::ok("duplicate, already processed");
    }

    // 5. Dispatch
    let outcome = match event.event_type.as_str() {
        "checkout.session.completed" => handle_checkout_session_completed(&ctx.env, &db, &event).await,
        "customer.subscription.updated" => handle_subscription_updated(&ctx.env, &db, &event).await,
        "customer.subscription.deleted" => handle_subscription_deleted(&db, &event).await,
        other => {
            console_log!("stripe webhook unhandled event type: {other}");
            Ok(())
        }
    };

    // 6. Record processed event (whether handler succeeded or not) so we
    //    don't retry infinitely on a persistent bug — surface via logs
    let now_iso = worker::Date::now().to_string();
    db.prepare("INSERT INTO webhook_events (stripe_event_id, event_type, processed_at) VALUES (?, ?, ?)")
        .bind(&[
            event.id.clone().into(),
            event.event_type.clone().into(),
            now_iso.into(),
        ])?
        .run()
        .await?;

    match outcome {
        Ok(()) => Response::ok("processed"),
        Err(e) => {
            console_log!("stripe webhook handler error ({}): {e}", event.event_type);
            // 200 anyway so Stripe stops retrying — operator investigates via logs
            Response::ok("logged")
        }
    }
}

async fn handle_checkout_session_completed(
    env: &Env,
    db: &D1Database,
    event: &StripeEvent,
) -> Result<(), String> {
    let session = &event.data.object;
    let customer_id = session
        .get("customer")
        .and_then(|v| v.as_str())
        .ok_or("checkout.session.completed missing customer id")?
        .to_string();
    let email = session
        .get("customer_details")
        .and_then(|v| v.get("email"))
        .and_then(|v| v.as_str())
        .or_else(|| session.get("customer_email").and_then(|v| v.as_str()))
        .ok_or("checkout.session.completed missing email")?
        .to_string();
    let subscription_id = session
        .get("subscription")
        .and_then(|v| v.as_str())
        .map(String::from);

    // The session doesn't include the price directly; look it up from the
    // subscription lines. For Phase S1 we accept the metadata hint instead
    // to keep the webhook self-contained (Phase S2 adds the Stripe API
    // callback path)
    let price_id = session
        .get("metadata")
        .and_then(|v| v.get("price_id"))
        .and_then(|v| v.as_str())
        .ok_or("checkout.session.completed missing metadata.price_id (set from checkout create call)")?
        .to_string();

    let tier = tier_for_price_id(env, &price_id)
        .ok_or_else(|| format!("unknown price id {price_id} — add to PRICE_ID_* env vars"))?;

    upsert_subscriber(
        db,
        &customer_id,
        &email,
        subscription_id.as_deref(),
        &price_id,
        &tier,
        "active",
        None,
    )
    .await?;

    // Issue an initial license — 33 days validity (30 day subscription +
    // 3 day grace); subscription.updated will re-issue with exact
    // current_period_end once Stripe sends that event (usually within
    // seconds after checkout.session.completed)
    issue_and_deliver_license(env, db, &customer_id, &email, &tier, 33).await?;

    Ok(())
}

async fn handle_subscription_updated(
    env: &Env,
    db: &D1Database,
    event: &StripeEvent,
) -> Result<(), String> {
    let sub = &event.data.object;
    let customer_id = sub
        .get("customer")
        .and_then(|v| v.as_str())
        .ok_or("subscription.updated missing customer id")?
        .to_string();
    let subscription_id = sub
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("subscription.updated missing id")?
        .to_string();
    let status = sub
        .get("status")
        .and_then(|v| v.as_str())
        .ok_or("subscription.updated missing status")?
        .to_string();
    let price_id = sub
        .get("items")
        .and_then(|v| v.get("data"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|item| item.get("price"))
        .and_then(|p| p.get("id"))
        .and_then(|v| v.as_str())
        .ok_or("subscription.updated missing items[0].price.id")?
        .to_string();
    let current_period_end_unix = sub
        .get("current_period_end")
        .and_then(|v| v.as_i64())
        .ok_or("subscription.updated missing current_period_end")?;
    let current_period_end: DateTime<Utc> = Utc
        .timestamp_opt(current_period_end_unix, 0)
        .single()
        .ok_or("subscription.updated current_period_end out of range")?;

    let tier = tier_for_price_id(env, &price_id)
        .ok_or_else(|| format!("unknown price id {price_id}"))?;

    // Look up email from the existing subscriber row
    let existing = db
        .prepare("SELECT email FROM subscribers WHERE stripe_customer_id = ?")
        .bind(&[customer_id.clone().into()])
        .map_err(|e| format!("prepare: {e}"))?
        .first::<serde_json::Value>(None)
        .await
        .map_err(|e| format!("query: {e}"))?;
    let email = existing
        .as_ref()
        .and_then(|v| v.get("email"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("no existing subscriber for customer {customer_id}"))?
        .to_string();

    upsert_subscriber(
        db,
        &customer_id,
        &email,
        Some(&subscription_id),
        &price_id,
        &tier,
        &status,
        Some(&current_period_end),
    )
    .await?;

    // Re-issue license only for active/trialing subscriptions — canceled
    // states just persist the status change
    if matches!(status.as_str(), "active" | "trialing") {
        let expires_at = current_period_end + chrono::Duration::days(3);
        let issuer = load_issuer(env)?;
        let key = issuer
            .issue_until(&tier, &customer_id, expires_at)
            .map_err(|e| format!("license issue: {e}"))?;
        let encoded = key.to_base64().map_err(|e| format!("license encode: {e}"))?;
        insert_license(db, &customer_id, &encoded, &tier, &key.payload.issued_at, &expires_at).await?;
        email::send_license_email(env, &email, &encoded, &tier).await;
    }

    Ok(())
}

async fn handle_subscription_deleted(db: &D1Database, event: &StripeEvent) -> Result<(), String> {
    let sub = &event.data.object;
    let customer_id = sub
        .get("customer")
        .and_then(|v| v.as_str())
        .ok_or("subscription.deleted missing customer id")?
        .to_string();

    let now_iso = worker::Date::now().to_string();
    db.prepare("UPDATE subscribers SET status = 'canceled', updated_at = ? WHERE stripe_customer_id = ?")
        .bind(&[now_iso.clone().into(), customer_id.clone().into()])
        .map_err(|e| format!("prepare: {e}"))?
        .run()
        .await
        .map_err(|e| format!("update subscriber: {e}"))?;
    db.prepare("UPDATE licenses SET revoked_at = ? WHERE stripe_customer_id = ? AND revoked_at IS NULL")
        .bind(&[now_iso.into(), customer_id.into()])
        .map_err(|e| format!("prepare: {e}"))?
        .run()
        .await
        .map_err(|e| format!("revoke licenses: {e}"))?;
    Ok(())
}

fn load_issuer(env: &Env) -> Result<LicenseIssuer, String> {
    let secret = env
        .secret("LICENSE_SIGNING_KEY_HEX")
        .map_err(|_| "LICENSE_SIGNING_KEY_HEX not configured".to_string())?
        .to_string();
    LicenseIssuer::from_hex(&secret).map_err(|e| format!("license issuer: {e}"))
}

#[allow(clippy::too_many_arguments)]
async fn upsert_subscriber(
    db: &D1Database,
    customer_id: &str,
    email: &str,
    subscription_id: Option<&str>,
    price_id: &str,
    tier: &str,
    status: &str,
    current_period_end: Option<&DateTime<Utc>>,
) -> Result<(), String> {
    let now_iso = worker::Date::now().to_string();
    let period_end_str = current_period_end.map(|d| d.to_rfc3339()).unwrap_or_default();
    db.prepare(
        "INSERT INTO subscribers (stripe_customer_id, email, stripe_subscription_id, stripe_price_id, tier, status, current_period_end, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(stripe_customer_id) DO UPDATE SET
           email = excluded.email,
           stripe_subscription_id = excluded.stripe_subscription_id,
           stripe_price_id = excluded.stripe_price_id,
           tier = excluded.tier,
           status = excluded.status,
           current_period_end = excluded.current_period_end,
           updated_at = excluded.updated_at",
    )
    .bind(&[
        customer_id.into(),
        email.into(),
        subscription_id.unwrap_or("").into(),
        price_id.into(),
        tier.into(),
        status.into(),
        period_end_str.into(),
        now_iso.clone().into(),
        now_iso.into(),
    ])
    .map_err(|e| format!("prepare: {e}"))?
    .run()
    .await
    .map_err(|e| format!("upsert subscriber: {e}"))?;
    Ok(())
}

async fn issue_and_deliver_license(
    env: &Env,
    db: &D1Database,
    customer_id: &str,
    email: &str,
    tier: &str,
    valid_days: i64,
) -> Result<(), String> {
    let issuer = load_issuer(env)?;
    let key = issuer
        .issue(tier, customer_id, valid_days)
        .map_err(|e| format!("license issue: {e}"))?;
    let encoded = key.to_base64().map_err(|e| format!("license encode: {e}"))?;
    insert_license(
        db,
        customer_id,
        &encoded,
        tier,
        &key.payload.issued_at,
        &key.payload.expires_at,
    )
    .await?;
    email::send_license_email(env, email, &encoded, tier).await;
    Ok(())
}

async fn insert_license(
    db: &D1Database,
    customer_id: &str,
    license_key: &str,
    tier: &str,
    issued_at: &DateTime<Utc>,
    expires_at: &DateTime<Utc>,
) -> Result<(), String> {
    let id = uuid::Uuid::new_v4().to_string();
    db.prepare(
        "INSERT INTO licenses (id, stripe_customer_id, license_key, tier, issued_at, expires_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&[
        id.into(),
        customer_id.into(),
        license_key.into(),
        tier.into(),
        issued_at.to_rfc3339().into(),
        expires_at.to_rfc3339().into(),
    ])
    .map_err(|e| format!("prepare: {e}"))?
    .run()
    .await
    .map_err(|e| format!("insert license: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fixed test vectors so signature verification is deterministic
    const TEST_SECRET: &str = "whsec_test_secret_do_not_use_in_production";
    const TEST_PAYLOAD: &str = r#"{"id":"evt_1","type":"checkout.session.completed"}"#;
    const TEST_TIMESTAMP: i64 = 1_700_000_000;

    fn compute_signature(payload: &str, ts: i64, secret: &str) -> String {
        let signed = format!("{ts}.{payload}");
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(signed.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    #[test]
    fn verify_signature_accepts_valid() {
        let sig = compute_signature(TEST_PAYLOAD, TEST_TIMESTAMP, TEST_SECRET);
        let header = format!("t={TEST_TIMESTAMP},v1={sig}");
        assert!(verify_signature(TEST_PAYLOAD, &header, TEST_SECRET, 300, TEST_TIMESTAMP).is_ok());
    }

    #[test]
    fn verify_signature_rejects_wrong_secret() {
        let sig = compute_signature(TEST_PAYLOAD, TEST_TIMESTAMP, TEST_SECRET);
        let header = format!("t={TEST_TIMESTAMP},v1={sig}");
        assert_eq!(
            verify_signature(TEST_PAYLOAD, &header, "wrong-secret", 300, TEST_TIMESTAMP),
            Err(WebhookError::SignatureMismatch)
        );
    }

    #[test]
    fn verify_signature_rejects_stale_timestamp() {
        let sig = compute_signature(TEST_PAYLOAD, TEST_TIMESTAMP, TEST_SECRET);
        let header = format!("t={TEST_TIMESTAMP},v1={sig}");
        // now is 10 minutes after signed timestamp, tolerance 5 minutes
        assert_eq!(
            verify_signature(TEST_PAYLOAD, &header, TEST_SECRET, 300, TEST_TIMESTAMP + 600),
            Err(WebhookError::TimestampOutOfTolerance)
        );
    }

    #[test]
    fn verify_signature_rejects_tampered_payload() {
        let sig = compute_signature(TEST_PAYLOAD, TEST_TIMESTAMP, TEST_SECRET);
        let header = format!("t={TEST_TIMESTAMP},v1={sig}");
        let tampered = TEST_PAYLOAD.replace("checkout", "chek0ut");
        assert_eq!(
            verify_signature(&tampered, &header, TEST_SECRET, 300, TEST_TIMESTAMP),
            Err(WebhookError::SignatureMismatch)
        );
    }

    #[test]
    fn verify_signature_accepts_multiple_v1_entries_during_rotation() {
        let sig_a = compute_signature(TEST_PAYLOAD, TEST_TIMESTAMP, "old-secret");
        let sig_b = compute_signature(TEST_PAYLOAD, TEST_TIMESTAMP, TEST_SECRET);
        let header = format!("t={TEST_TIMESTAMP},v1={sig_a},v1={sig_b}");
        assert!(verify_signature(TEST_PAYLOAD, &header, TEST_SECRET, 300, TEST_TIMESTAMP).is_ok());
    }

    #[test]
    fn verify_signature_rejects_malformed_header() {
        assert_eq!(
            verify_signature(TEST_PAYLOAD, "no-equals-sign", TEST_SECRET, 300, TEST_TIMESTAMP),
            Err(WebhookError::MalformedSignature)
        );
    }

    #[test]
    fn verify_signature_rejects_no_v1_entry() {
        let header = format!("t={TEST_TIMESTAMP},v0=notarealscheme");
        assert_eq!(
            verify_signature(TEST_PAYLOAD, &header, TEST_SECRET, 300, TEST_TIMESTAMP),
            Err(WebhookError::NoV1Signature)
        );
    }
}
