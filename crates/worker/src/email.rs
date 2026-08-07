//! Resend API adapter for license key delivery (Phase S1)
//!
//! Uses the Resend API (https://resend.com/docs/api-reference/emails/send-email)
//! because it has a Rust-friendly REST surface and a generous free tier
//! (100 emails/day)
//!
//! Behavior:
//! - `RESEND_API_KEY` + `LICENSE_FROM_EMAIL` configured → send real email
//! - Either missing → log the license key + intended recipient to
//!   `console_log` and return Ok The license row is still persisted in
//!   D1 so the operator can retrieve it manually
//!
//! This lets Phase S1 ship without a Resend account; Phase S2 flips the
//! switch by adding the two secrets

use serde_json::json;
use worker::{console_log, Env, Fetch, Headers, Method, Request, RequestInit};

/// Never returns Err — email failures must not roll back the license
/// issuance (the operator can always redeliver from the D1 row)
pub async fn send_license_email(env: &Env, to: &str, license_key: &str, tier: &str) {
    let api_key = match env.secret("RESEND_API_KEY").ok().map(|v| v.to_string()) {
        Some(k) if !k.is_empty() => k,
        _ => {
            console_log!(
                "[email stub] would send license to {to} tier={tier} key_len={}",
                license_key.len()
            );
            return;
        }
    };
    let from = env
        .var("LICENSE_FROM_EMAIL")
        .ok()
        .map(|v| v.to_string())
        .unwrap_or_else(|| "no-reply@text-to-print.alicelaw.net".to_string());

    let body = build_email_body(&from, to, license_key, tier);
    let payload = match serde_json::to_string(&body) {
        Ok(s) => s,
        Err(e) => {
            console_log!("[email] failed to serialize body: {e}");
            return;
        }
    };

    let mut headers = Headers::new();
    if headers.set("Authorization", &format!("Bearer {api_key}")).is_err() {
        console_log!("[email] failed to set Authorization header");
        return;
    }
    if headers.set("Content-Type", "application/json").is_err() {
        console_log!("[email] failed to set Content-Type header");
        return;
    }

    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(payload.into()));

    let req = match Request::new_with_init("https://api.resend.com/emails", &init) {
        Ok(r) => r,
        Err(e) => {
            console_log!("[email] failed to build request: {e}");
            return;
        }
    };

    match Fetch::Request(req).send().await {
        Ok(mut resp) => {
            let status = resp.status_code();
            if !(200..300).contains(&status) {
                let text = resp.text().await.unwrap_or_default();
                console_log!("[email] resend api {status}: {text}");
            } else {
                console_log!("[email] license delivered to {to} tier={tier}");
            }
        }
        Err(e) => console_log!("[email] resend fetch error: {e}"),
    }
}

/// Build the Resend `POST /emails` request body Kept as a pure function
/// so `#[test]` verifies the wire format without live network calls
pub fn build_email_body(from: &str, to: &str, license_key: &str, tier: &str) -> serde_json::Value {
    let subject = format!("text-to-print {tier} license key");
    let text = format!(
        "text-to-print をご利用いただきありがとうございます\n\n\
         下記のライセンスキーを アプリの Settings > Enter License Key に貼付してください\n\n\
         Tier: {tier}\n\n\
         ── License key ──\n{license_key}\n────────────────\n\n\
         有効期限内は継続的に自動更新されます 課金 subscription を解約すると\n\
         次回更新月末で Free tier に自動 rollback します\n\n\
         サポート: support@alicelaw.net\n"
    );
    json!({
        "from": from,
        "to": [to],
        "subject": subject,
        "text": text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_body_shape_matches_resend_api() {
        let body = build_email_body(
            "no-reply@text-to-print.alicelaw.net",
            "user@example.com",
            "eyJwYXlsb2FkIjp7InRpZXIiOiJQcm8ifX0=",
            "Pro",
        );
        assert_eq!(body["from"], "no-reply@text-to-print.alicelaw.net");
        assert_eq!(body["to"][0], "user@example.com");
        assert_eq!(body["subject"], "text-to-print Pro license key");
        let text = body["text"].as_str().unwrap();
        assert!(text.contains("eyJwYXlsb2FkIjp7InRpZXIiOiJQcm8ifX0="));
        assert!(text.contains("Tier: Pro"));
    }

    #[test]
    fn email_body_never_leaks_signing_key_material() {
        // Sanity: `license_key` is what we do send, but the `from` and `to`
        // should never accidentally embed anything else
        let body = build_email_body("a@b.c", "u@v.w", "LICENSE_HERE", "Pro");
        let serialized = serde_json::to_string(&body).unwrap();
        // No stray strings that suggest key material other than the license
        assert!(!serialized.contains("SIGNING"));
        assert!(!serialized.contains("SECRET"));
        assert!(serialized.contains("LICENSE_HERE"));
    }
}
