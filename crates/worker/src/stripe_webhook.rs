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

/// What a completed checkout session allows
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckoutDecision {
    /// paid, or no payment required: issue the licence now
    Issue,
    /// not paid yet (a delayed payment method): wait for
    /// `checkout.session.async_payment_succeeded`
    AwaitPayment,
}

/// `payment_status` `paid` or `no_payment_required` issues; anything else waits
#[must_use]
pub fn checkout_decision(session: &serde_json::Value) -> CheckoutDecision {
    match session.get("payment_status").and_then(|v| v.as_str()) {
        Some("paid" | "no_payment_required") => CheckoutDecision::Issue,
        _ => CheckoutDecision::AwaitPayment,
    }
}

/// `(record the event as processed, HTTP status)` for a handler outcome: a failure
/// is not recorded and answers 500, so Stripe delivers the event again
#[must_use]
pub fn webhook_outcome(outcome: &Result<(), String>) -> (bool, u16) {
    match outcome {
        Ok(()) => (true, 200),
        Err(_) => (false, 500),
    }
}

/// The issuance key of the licence for one checkout session
#[must_use]
pub fn checkout_issue_key(session_id: &str) -> String {
    format!("checkout:{session_id}")
}

/// The issuance key of the licence for one billing period of a subscription
#[must_use]
pub fn subscription_issue_key(subscription_id: &str, period_end_unix: i64) -> String {
    format!("subscription:{subscription_id}:{period_end_unix}")
}

/// Store a licence under its issuance key; a second attempt with the same key
/// leaves the first row as it is (`migrations/0005_license_issue_key.sql`)
pub const INSERT_LICENSE_SQL: &str =
    "INSERT INTO licenses (id, stripe_customer_id, license_key, tier, issued_at, expires_at, issue_key)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
     ON CONFLICT(issue_key) WHERE issue_key IS NOT NULL DO NOTHING";

/// The licence stored under an issuance key (the one to deliver)
pub const SELECT_ISSUED_LICENSE_SQL: &str =
    "SELECT license_key FROM licenses WHERE issue_key = ?1";

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
        "checkout.session.completed" => {
            if checkout_decision(&event.data.object) == CheckoutDecision::Issue {
                handle_checkout_session_completed(&ctx.env, &db, &event).await
            } else {
                console_log!("stripe webhook: checkout {} not paid yet, waiting", event.id);
                Ok(())
            }
        }
        // a delayed payment settled: the session is now paid
        "checkout.session.async_payment_succeeded" => {
            handle_checkout_session_completed(&ctx.env, &db, &event).await
        }
        "checkout.session.async_payment_failed" => {
            console_log!("stripe webhook: checkout {} payment failed, no licence", event.id);
            Ok(())
        }
        "customer.subscription.updated" => handle_subscription_updated(&ctx.env, &db, &event).await,
        "customer.subscription.deleted" => handle_subscription_deleted(&db, &event).await,
        other => {
            console_log!("stripe webhook unhandled event type: {other}");
            Ok(())
        }
    };

    // 6. Record the event only when the handler succeeded; a failure answers 500
    //    so Stripe delivers it again (issuance is idempotent per issue_key, so a
    //    retry after a partial success resends the stored key)
    let (record, status) = webhook_outcome(&outcome);
    if record {
        let now_iso = worker::Date::now().to_string();
        db.prepare("INSERT INTO webhook_events (stripe_event_id, event_type, processed_at) VALUES (?, ?, ?)")
            .bind(&[
                event.id.clone().into(),
                event.event_type.clone().into(),
                now_iso.into(),
            ])?
            .run()
            .await?;
        return Response::ok("processed");
    }
    if let Err(e) = outcome {
        console_log!("stripe webhook handler error ({}): {e}", event.event_type);
    }
    Response::error("handler failed, retry", status)
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
    let session_id = session
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("checkout.session.completed missing session id")?
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
    let issuer = load_issuer(env)?;
    let key = issuer
        .issue(&tier, &customer_id, 33)
        .map_err(|e| format!("license issue: {e}"))?;
    let encoded = key.to_base64().map_err(|e| format!("license encode: {e}"))?;
    let stored = store_license_once(
        db,
        &checkout_issue_key(&session_id),
        &customer_id,
        &encoded,
        &tier,
        &key.payload.issued_at,
        &key.payload.expires_at,
    )
    .await?;
    email::send_license_email(env, &email, &stored, &tier).await?;

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
        let stored = store_license_once(
            db,
            &subscription_issue_key(&subscription_id, current_period_end_unix),
            &customer_id,
            &encoded,
            &tier,
            &key.payload.issued_at,
            &expires_at,
        )
        .await?;
        email::send_license_email(env, &email, &stored, &tier).await?;
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

/// Store `license_key` under `issue_key` unless a licence is already stored there,
/// and return the stored one (the key to deliver: a retry resends the first key)
async fn store_license_once(
    db: &D1Database,
    issue_key: &str,
    customer_id: &str,
    license_key: &str,
    tier: &str,
    issued_at: &DateTime<Utc>,
    expires_at: &DateTime<Utc>,
) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    db.prepare(INSERT_LICENSE_SQL)
        .bind(&[
            id.into(),
            customer_id.into(),
            license_key.into(),
            tier.into(),
            issued_at.to_rfc3339().into(),
            expires_at.to_rfc3339().into(),
            issue_key.into(),
        ])
        .map_err(|e| format!("prepare: {e}"))?
        .run()
        .await
        .map_err(|e| format!("insert license: {e}"))?;
    let row = db
        .prepare(SELECT_ISSUED_LICENSE_SQL)
        .bind(&[issue_key.into()])
        .map_err(|e| format!("prepare: {e}"))?
        .first::<serde_json::Value>(None)
        .await
        .map_err(|e| format!("select license: {e}"))?;
    row.as_ref()
        .and_then(|v| v.get("license_key"))
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| format!("no licence stored under {issue_key}"))
}

#[cfg(test)]
#[path = "webhook_issue_tests.rs"]
mod webhook_issue_tests;

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
