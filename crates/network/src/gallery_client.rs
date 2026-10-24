//! Client for the Cloudflare-relayed Gallery (Phase 3、2026-08-26)
//!
//! Corresponds to the Worker endpoints in `crates/worker/src/gallery_handler.rs`:
//! - `GET  /api/gallery/list`
//! - `POST /api/gallery/publish`
//! - `DELETE /api/gallery/{id}`
//!
//! ## Signing
//!
//! Publish + delete both require an ed25519 signature over a canonical
//! message that is separator-distinct so a captured publish sig can not
//! be replayed as a delete Sign with the caller's `crate::Identity`
//! and pass the hex-encoded sig into `GalleryClient::publish` /
//! `GalleryClient::delete`

use anyhow::{Context, Result};
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GalleryItem {
    pub id: String,
    pub author_did: String,
    pub author_nickname: Option<String>,
    pub lol_source: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResponse {
    pub items: Vec<GalleryItem>,
    pub next_offset: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
struct PublishRequest {
    id: String,
    author_did: String,
    author_nickname: Option<String>,
    lol_source: String,
    created_at: String,
    signature: String,
}

/// Every field a Gallery publish request carries, as JSON keys
///
/// Read from the serialised request, so it follows `PublishRequest` the
/// same way [`crate::share::sent_field_names`] follows the share payload
#[must_use]
pub fn publish_field_names() -> Vec<String> {
    let full = PublishRequest {
        id: String::new(),
        author_did: String::new(),
        author_nickname: Some(String::new()),
        lol_source: String::new(),
        created_at: String::new(),
        signature: String::new(),
    };
    let value = serde_json::to_value(full).expect("PublishRequest serialises to JSON");
    crate::share::json_leaf_paths(&value)
}

#[derive(Debug, Clone, Serialize)]
struct DeleteRequest {
    author_did: String,
    signature: String,
}

pub struct GalleryClient {
    endpoint: String,
    client: reqwest::Client,
}

impl GalleryClient {
    /// Build a client Endpoint is the base URL (e.g.
    /// `https://text-to-print.alicelaw.net/api/gallery`) — the client
    /// appends `/list` / `/publish` / `/{id}` at request time
    #[must_use]
    pub fn new(endpoint: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { endpoint, client }
    }

    /// Default endpoint on production Cloudflare Worker Uses the same
    /// custom domain as `/api/presets` and `/api/share`
    #[must_use]
    pub fn default_endpoint() -> String {
        "https://text-to-print.alicelaw.net/api/gallery".to_string()
    }

    /// Resolve the effective endpoint by precedence:
    ///   1. `TTP_GALLERY_ENDPOINT` env (dev / one-shot override)
    ///   2. `default_endpoint()`
    #[must_use]
    pub fn resolve_endpoint() -> String {
        if let Ok(url) = std::env::var("TTP_GALLERY_ENDPOINT")
            && !url.trim().is_empty()
        {
            return url;
        }
        Self::default_endpoint()
    }

    /// Fetch the latest-first Gallery listing
    ///
    /// # Errors
    /// Network, non-2xx status, or JSON parse failure
    pub async fn list(&self, limit: u32, offset: u32) -> Result<ListResponse> {
        let url = format!("{}/list?limit={}&offset={}", self.endpoint, limit, offset);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("gallery list failed: {status} — {body}");
        }
        resp.json::<ListResponse>()
            .await
            .with_context(|| "parse gallery list JSON")
    }

    /// Publish a Gallery post signs the canonical message with the
    /// supplied [`SigningKey`] and forwards to the Worker for validate +
    /// insert The `id` should be a fresh UUID v7 (`uuid::Uuid::now_v7()`)
    /// The `created_at` should be `chrono::Utc::now().to_rfc3339()`
    ///
    /// # Errors
    /// - Network or non-2xx status
    /// - Server-side reject (return body is echoed in the error message)
    pub async fn publish(
        &self,
        signing_key: &SigningKey,
        id: &str,
        author_did: &str,
        author_nickname: Option<&str>,
        lol_source: &str,
        created_at: &str,
    ) -> Result<()> {
        let body = signed_publish_request(
            signing_key,
            id,
            author_did,
            author_nickname,
            lol_source,
            created_at,
        )?;
        let url = format!("{}/publish", self.endpoint);
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("POST {url}"))?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("gallery publish failed: {status} — {text}");
        }
        Ok(())
    }

    /// Delete own-only post signs a delete-scoped canonical message so
    /// a captured publish sig cannot be replayed as a delete
    ///
    /// # Errors
    /// - Network or non-2xx status
    /// - 404 (row not found or ownership mismatch — both collapsed by
    ///   the Worker to avoid ownership info leak)
    pub async fn delete(&self, signing_key: &SigningKey, id: &str, author_did: &str) -> Result<()> {
        let body = signed_delete_request(signing_key, id, author_did)?;
        let url = format!("{}/{}", self.endpoint, id);
        let resp = self
            .client
            .delete(&url)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("DELETE {url}"))?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("gallery delete failed: {status} — {text}");
        }
        Ok(())
    }
}

/// The `POST /publish` body: the fields as given and their signature
fn signed_publish_request(
    signing_key: &SigningKey,
    id: &str,
    author_did: &str,
    author_nickname: Option<&str>,
    lol_source: &str,
    created_at: &str,
) -> Result<PublishRequest> {
    let message = publish_message(id, author_did, author_nickname, lol_source, created_at)?;
    Ok(PublishRequest {
        id: id.to_string(),
        author_did: author_did.to_string(),
        author_nickname: author_nickname.map(str::to_string),
        lol_source: lol_source.to_string(),
        created_at: created_at.to_string(),
        signature: hex::encode(text_to_print_gallery_sign::sign(
            signing_key.to_bytes(),
            &message,
        )),
    })
}

/// The `DELETE /{id}` body for the post `id`: its author and their signature
fn signed_delete_request(
    signing_key: &SigningKey,
    id: &str,
    author_did: &str,
) -> Result<DeleteRequest> {
    let message = delete_message(id, author_did)?;
    Ok(DeleteRequest {
        author_did: author_did.to_string(),
        signature: hex::encode(text_to_print_gallery_sign::sign(
            signing_key.to_bytes(),
            &message,
        )),
    })
}

/// The bytes the client signs for `POST /publish`: every field of the request,
/// built by the same code the Worker verifies with
///
/// # Errors
/// A field longer than the encoding allows
pub fn publish_message(
    id: &str,
    author_did: &str,
    author_nickname: Option<&str>,
    lol_source: &str,
    created_at: &str,
) -> Result<Vec<u8>> {
    text_to_print_gallery_sign::publish_message(&text_to_print_gallery_sign::PublishFields {
        id,
        author_did,
        author_nickname,
        lol_source,
        created_at,
    })
    .map_err(|e| anyhow::anyhow!("gallery publish message: {e:?}"))
}

/// The bytes the client signs for `DELETE /{id}`; its domain differs from
/// publish, so a captured publish signature can not be replayed as a delete
///
/// # Errors
/// A field longer than the encoding allows
pub fn delete_message(id: &str, author_did: &str) -> Result<Vec<u8>> {
    text_to_print_gallery_sign::delete_message(id, author_did)
        .map_err(|e| anyhow::anyhow!("gallery delete message: {e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex_of(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// The client's messages are the exact bytes the Worker verifies; the
    /// fields are distinct so a swapped argument changes the bytes (oracle:
    /// the bytes pinned in crates/gallery-sign `the_message_bytes_are_fixed`)
    #[test]
    fn the_client_messages_are_the_pinned_bytes() {
        let publish = publish_message(
            "018f4e8c-0000-7000-8000-000000000001",
            "did:key:aa",
            Some("maker"),
            "cube(10)",
            "2026-10-24T00:00:00Z",
        )
        .unwrap();
        assert_eq!(
            hex_of(&publish),
            "414c53520100dd00000006\
             db00000020746578742d746f2d7072696e742067616c6c657279207075626c697368207631\
             db0000002430313866346538632d303030302d373030302d383030302d303030303030303030303031\
             db0000000a6469643a6b65793a6161\
             db000000056d616b6572\
             db000000086375626528313029\
             db00000014323032362d31302d32345430303a30303a30305a"
        );
        assert_eq!(
            hex_of(&delete_message("id1", "did:key:aa").unwrap()),
            "414c53520100dd00000003\
             db0000001f746578742d746f2d7072696e742067616c6c6572792064656c657465207631\
             db00000003696431\
             db0000000a6469643a6b65793a6161"
        );
    }

    fn signature_of(hex_sig: &str) -> [u8; 64] {
        let mut out = [0u8; 64];
        hex::decode_to_slice(hex_sig, &mut out).unwrap();
        out
    }

    /// The bodies the app sends verify, under the Worker's check, over the
    /// message of the fields they carry, so a request built with its fields in
    /// another order is refused here as it would be there
    #[test]
    fn the_request_bodies_carry_what_was_signed() {
        let key = SigningKey::from_bytes(&[11u8; 32]);
        let public = key.verifying_key().to_bytes();
        let did = format!("did:key:{}", hex::encode(public));
        let id = "018f4e8c-0000-7000-8000-000000000001";

        let p = signed_publish_request(
            &key,
            id,
            &did,
            Some("maker"),
            "cube(10)",
            "2026-10-24T00:00:00Z",
        )
        .unwrap();
        let expected = text_to_print_gallery_sign::publish_message(
            &text_to_print_gallery_sign::PublishFields {
                id: &p.id,
                author_did: &p.author_did,
                author_nickname: p.author_nickname.as_deref(),
                lol_source: &p.lol_source,
                created_at: &p.created_at,
            },
        )
        .unwrap();
        assert_eq!(
            (
                p.id.as_str(),
                p.author_did.as_str(),
                p.author_nickname.as_deref(),
                p.lol_source.as_str(),
                p.created_at.as_str()
            ),
            (
                id,
                did.as_str(),
                Some("maker"),
                "cube(10)",
                "2026-10-24T00:00:00Z"
            )
        );
        assert!(text_to_print_gallery_sign::verify(
            public,
            &expected,
            signature_of(&p.signature)
        ));

        let d = signed_delete_request(&key, id, &did).unwrap();
        assert_eq!(d.author_did, did);
        let expected = text_to_print_gallery_sign::delete_message(id, &did).unwrap();
        assert!(text_to_print_gallery_sign::verify(
            public,
            &expected,
            signature_of(&d.signature)
        ));
    }

    #[test]
    fn publish_message_covers_the_nickname() {
        let id = "018f4e8c-0000-7000-8000-000000000001";
        let with = publish_message(
            id,
            "did:key:aa",
            Some("maker"),
            "cube(1)",
            "2026-08-26T12:00:00Z",
        )
        .unwrap();
        let without =
            publish_message(id, "did:key:aa", None, "cube(1)", "2026-08-26T12:00:00Z").unwrap();
        assert_ne!(with, without);
    }

    #[test]
    fn delete_message_differs_from_publish() {
        let delete = delete_message("id1", "did:key:aa").unwrap();
        let publish = publish_message("id1", "did:key:aa", None, "any lol", "any ts").unwrap();
        assert_ne!(delete, publish);
    }

    #[test]
    fn gallery_client_uses_default_endpoint() {
        assert_eq!(
            GalleryClient::default_endpoint(),
            "https://text-to-print.alicelaw.net/api/gallery"
        );
    }

    #[test]
    fn resolve_endpoint_returns_default_when_env_unset() {
        unsafe {
            std::env::remove_var("TTP_GALLERY_ENDPOINT");
        }
        assert_eq!(
            GalleryClient::resolve_endpoint(),
            GalleryClient::default_endpoint()
        );
    }
}
