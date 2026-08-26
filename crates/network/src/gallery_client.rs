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
use ed25519_dalek::{Signer, SigningKey};
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
        let canonical = publish_canonical(id, author_did, lol_source, created_at);
        let sig = signing_key.sign(canonical.as_bytes());
        let sig_hex = hex::encode(sig.to_bytes());
        let body = PublishRequest {
            id: id.to_string(),
            author_did: author_did.to_string(),
            author_nickname: author_nickname.map(str::to_string),
            lol_source: lol_source.to_string(),
            created_at: created_at.to_string(),
            signature: sig_hex,
        };
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
        let canonical = delete_canonical(id, author_did);
        let sig = signing_key.sign(canonical.as_bytes());
        let sig_hex = hex::encode(sig.to_bytes());
        let body = DeleteRequest {
            author_did: author_did.to_string(),
            signature: sig_hex,
        };
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

/// Canonical message signed by the client for `POST /publish` Format
/// must match `crates/worker/src/validate.rs::gallery_publish_canonical`
#[must_use]
pub fn publish_canonical(id: &str, author_did: &str, lol_source: &str, created_at: &str) -> String {
    format!("{id}|{author_did}|{lol_source}|{created_at}")
}

/// Canonical message signed by the client for `DELETE /{id}` Format
/// must match `crates/worker/src/validate.rs::gallery_delete_canonical`
/// The `delete|` prefix keeps this distinct from publish so a captured
/// publish sig can not be replayed as a delete of the same row
#[must_use]
pub fn delete_canonical(id: &str, author_did: &str) -> String {
    format!("delete|{id}|{author_did}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_canonical_format_stable() {
        // Wire compat: worker validate.rs must produce the same string
        let msg = publish_canonical(
            "018f4e8c-0000-7000-8000-000000000001",
            "did:key:aa",
            "cube(1)",
            "2026-08-26T12:00:00Z",
        );
        assert_eq!(
            msg,
            "018f4e8c-0000-7000-8000-000000000001|did:key:aa|cube(1)|2026-08-26T12:00:00Z"
        );
    }

    #[test]
    fn delete_canonical_has_prefix() {
        let msg = delete_canonical("id1", "did:key:aa");
        assert_eq!(msg, "delete|id1|did:key:aa");
        // Explicit replay-protection assertion: delete must never
        // collide with publish for the same (id, did) pair
        let publish = publish_canonical("id1", "did:key:aa", "any lol", "any ts");
        assert_ne!(msg, publish);
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
