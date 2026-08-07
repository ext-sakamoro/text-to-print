-- Phase S1: Stripe subscription + license issuance schema
--
-- Two tables:
--   subscribers  — Stripe customer + subscription state mirror
--   licenses     — Ed25519-signed license keys issued to subscribers
--
-- Written by `text-to-print-worker::stripe_webhook` on
-- `checkout.session.completed` / `customer.subscription.updated` /
-- `customer.subscription.deleted` events

CREATE TABLE IF NOT EXISTS subscribers (
    -- Stripe customer id (e.g. `cus_...`) — primary key so customer.subscription.*
    -- events can upsert by natural id without a separate uuid dance
    stripe_customer_id TEXT PRIMARY KEY,
    email TEXT NOT NULL,
    -- Latest Stripe subscription id (`sub_...`); null between checkout and
    -- first subscription webhook (rare but possible if events reorder)
    stripe_subscription_id TEXT,
    -- Latest Stripe price id (`price_...`) so we can distinguish monthly
    -- vs yearly billing without hardcoding tier logic in the DB
    stripe_price_id TEXT,
    -- Tier the current subscription grants (Pro / Enterprise); mirrors
    -- the `Tier` enum in `text_to_print_core::tier`
    tier TEXT NOT NULL,
    -- Subscription lifecycle: active / trialing / past_due / canceled / unpaid
    status TEXT NOT NULL,
    -- ISO 8601 UTC end of the current billing period; used to compute
    -- license expiry with a 3-day grace window
    current_period_end TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_subscribers_email ON subscribers(email);
CREATE INDEX IF NOT EXISTS idx_subscribers_status ON subscribers(status);

CREATE TABLE IF NOT EXISTS licenses (
    -- uuid v4, primary key
    id TEXT PRIMARY KEY,
    stripe_customer_id TEXT NOT NULL,
    -- Base64-encoded LicenseKey struct (`{ payload, signature }`) matching
    -- `text_to_print_core::license::LicenseKey::to_base64` output
    license_key TEXT NOT NULL,
    tier TEXT NOT NULL,
    issued_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    -- Non-null when the subscription was canceled / refunded / abused
    revoked_at TEXT,
    -- Delivery bookkeeping so we can retry email sends without duplicating
    email_delivered_at TEXT,
    FOREIGN KEY (stripe_customer_id) REFERENCES subscribers(stripe_customer_id)
);

CREATE INDEX IF NOT EXISTS idx_licenses_customer ON licenses(stripe_customer_id);
CREATE INDEX IF NOT EXISTS idx_licenses_expires ON licenses(expires_at);

-- Idempotency: Stripe delivers webhooks with at-least-once semantics, so
-- we dedupe by event id `evt_...` before applying state changes
CREATE TABLE IF NOT EXISTS webhook_events (
    stripe_event_id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    processed_at TEXT NOT NULL
);
