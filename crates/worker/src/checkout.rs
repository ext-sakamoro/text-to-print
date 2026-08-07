//! Stripe Checkout Session creation endpoint (Phase S1)
//!
//! `POST /stripe/checkout-session` body:
//! ```json
//! { "price_id": "price_...", "user_email": "user@example.com" }
//! ```
//!
//! Response:
//! ```json
//! { "url": "https://checkout.stripe.com/c/pay/..." }
//! ```
//!
//! The desktop app opens the returned URL in the user's browser. On
//! success/cancel, Stripe redirects to `CHECKOUT_SUCCESS_URL` /
//! `CHECKOUT_CANCEL_URL` (env-configured landing pages hosted on the
//! same domain — no cookies, purely informational)
//!
//! The `price_id` is passed as metadata so the webhook handler can
//! resolve tier without an extra Stripe API call

use serde::{Deserialize, Serialize};
use worker::{Fetch, Headers, Method, Request, RequestInit, Response, Result as WorkerResult, RouteContext, Url};

#[derive(Debug, Deserialize)]
pub struct CheckoutRequest {
    pub price_id: String,
    pub user_email: String,
}

#[derive(Debug, Serialize)]
pub struct CheckoutResponse {
    pub url: String,
    pub session_id: String,
}

pub async fn handle(req: &mut Request, ctx: &RouteContext<()>) -> WorkerResult<Response> {
    let body: CheckoutRequest = match req.json().await {
        Ok(v) => v,
        Err(e) => return Response::error(format!("invalid checkout request body: {e}"), 400),
    };

    // Basic input validation — Stripe rejects malformed values anyway but a
    // 400 here is more actionable for the client
    if !body.price_id.starts_with("price_") {
        return Response::error("price_id must start with 'price_'", 400);
    }
    if !body.user_email.contains('@') || body.user_email.len() > 320 {
        return Response::error("user_email is not a plausible address", 400);
    }

    let secret = ctx
        .env
        .secret("STRIPE_SECRET_KEY")
        .map_err(|_| worker::Error::from("STRIPE_SECRET_KEY not configured"))?
        .to_string();
    let success_url = ctx
        .env
        .var("CHECKOUT_SUCCESS_URL")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "https://text-to-print.alicelaw.net/checkout/success".to_string());
    let cancel_url = ctx
        .env
        .var("CHECKOUT_CANCEL_URL")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "https://text-to-print.alicelaw.net/checkout/cancel".to_string());

    let form_body = build_form_body(&body, &success_url, &cancel_url);

    let mut headers = Headers::new();
    headers.set("Authorization", &format!("Bearer {secret}"))?;
    headers.set("Content-Type", "application/x-www-form-urlencoded")?;

    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(form_body.into()));

    let stripe_req = Request::new_with_init(
        "https://api.stripe.com/v1/checkout/sessions",
        &init,
    )?;
    let mut stripe_resp = Fetch::Request(stripe_req).send().await?;

    let status = stripe_resp.status_code();
    let text = stripe_resp.text().await?;
    if !(200..300).contains(&status) {
        worker::console_log!("stripe checkout create failed status={status} body={text}");
        return Response::error(format!("stripe api error ({status}): {text}"), 502);
    }

    let session: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| worker::Error::from(format!("stripe response not json: {e}")))?;
    let url = session
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| worker::Error::from("stripe response missing url"))?
        .to_string();
    let session_id = session
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| worker::Error::from("stripe response missing id"))?
        .to_string();

    // Sanity check: ensure the returned URL is a Stripe-owned host
    if let Ok(parsed) = Url::parse(&url) {
        if !parsed
            .host_str()
            .map(|h| h.ends_with("stripe.com"))
            .unwrap_or(false)
        {
            return Response::error("stripe returned unexpected redirect host", 502);
        }
    }

    Response::from_json(&CheckoutResponse { url, session_id })
}

/// Build the `application/x-www-form-urlencoded` body Stripe expects
///
/// Keeping the encoding in a pure function so we can unit-test it without
/// mocking `Fetch`
pub fn build_form_body(req: &CheckoutRequest, success_url: &str, cancel_url: &str) -> String {
    // Order stable so tests can assert on it
    let mut parts: Vec<String> = Vec::new();
    parts.push("mode=subscription".to_string());
    parts.push(format!("success_url={}", urlencoding::encode(success_url)));
    parts.push(format!("cancel_url={}", urlencoding::encode(cancel_url)));
    parts.push(format!("customer_email={}", urlencoding::encode(&req.user_email)));
    parts.push(format!("line_items[0][price]={}", urlencoding::encode(&req.price_id)));
    parts.push("line_items[0][quantity]=1".to_string());
    // `price_id` echoed in session metadata so the webhook can resolve
    // tier without an extra API call to fetch the subscription
    parts.push(format!("metadata[price_id]={}", urlencoding::encode(&req.price_id)));
    parts.push("allow_promotion_codes=true".to_string());
    parts.join("&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_body_contains_required_fields() {
        let req = CheckoutRequest {
            price_id: "price_ABC123".to_string(),
            user_email: "user@example.com".to_string(),
        };
        let body = build_form_body(&req, "https://ok/success", "https://ok/cancel");
        assert!(body.contains("mode=subscription"));
        assert!(body.contains("line_items%5B0%5D%5Bprice%5D=price_ABC123") || body.contains("line_items[0][price]=price_ABC123"));
        assert!(body.contains("customer_email=user%40example.com"));
        assert!(body.contains("metadata%5Bprice_id%5D=price_ABC123") || body.contains("metadata[price_id]=price_ABC123"));
        assert!(body.contains("allow_promotion_codes=true"));
    }

    #[test]
    fn form_body_urlencodes_special_chars_in_email() {
        let req = CheckoutRequest {
            price_id: "price_1".to_string(),
            user_email: "user+tag@example.co.jp".to_string(),
        };
        let body = build_form_body(&req, "https://a", "https://b");
        // Both `+` and `@` should be percent-encoded in the body
        assert!(body.contains("customer_email=user%2Btag%40example.co.jp"));
    }

    #[test]
    fn form_body_urlencodes_success_cancel_urls() {
        let req = CheckoutRequest {
            price_id: "price_1".to_string(),
            user_email: "a@b.c".to_string(),
        };
        let body = build_form_body(
            &req,
            "https://text-to-print.alicelaw.net/checkout/success?src=app",
            "https://text-to-print.alicelaw.net/checkout/cancel",
        );
        assert!(body.contains("success_url=https%3A%2F%2Ftext-to-print.alicelaw.net%2Fcheckout%2Fsuccess%3Fsrc%3Dapp"));
    }
}
