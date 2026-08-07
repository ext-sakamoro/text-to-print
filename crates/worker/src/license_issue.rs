//! Ed25519 license issuance (worker-side)
//!
//! Independent implementation of the same signing scheme that
//! `text_to_print_core::license` uses on the desktop app. Both sides agree
//! on:
//!
//! 1. `LicensePayload` JSON structure — `{ tier, user_id, issued_at, expires_at }`
//!    with `issued_at` / `expires_at` as `DateTime<Utc>` RFC3339 strings
//! 2. Signature = Ed25519 over `serde_json::to_vec(&payload)`
//! 3. `LicenseKey` = `{ payload, signature: Vec<u8> }` JSON-serialized then
//!    base64-standard-encoded
//!
//! Design choice B (see `docs/STRIPE_SETUP.md`): the worker crate does not
//! depend on `text_to_print_core` to avoid dragging DB / IO layers into
//! wasm32 The wire format is contract-frozen; changes must go through
//! both crates and be covered by a round-trip test

use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};

/// Mirrors [`text_to_print_core::license::LicensePayload`]
///
/// `tier` must serialize to one of `"Free" | "General" | "Pro" | "Enterprise"`
/// to match the client-side [`text_to_print_core::tier::Tier`] enum
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicensePayload {
    pub tier: String,
    pub user_id: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// Mirrors [`text_to_print_core::license::LicenseKey`]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseKey {
    pub payload: LicensePayload,
    pub signature: Vec<u8>,
}

impl LicenseKey {
    /// Base64-standard-encoded JSON, matching
    /// [`text_to_print_core::license::LicenseKey::to_base64`]
    pub fn to_base64(&self) -> Result<String, LicenseError> {
        let bytes = serde_json::to_vec(self).map_err(LicenseError::Serialize)?;
        Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
    }
}

pub struct LicenseIssuer {
    signing_key: SigningKey,
}

impl LicenseIssuer {
    /// Load the Ed25519 signing key from a hex-encoded 32-byte secret
    /// (typically `wrangler secret put LICENSE_SIGNING_KEY_HEX`)
    pub fn from_hex(hex_secret: &str) -> Result<Self, LicenseError> {
        let bytes = hex::decode(hex_secret.trim()).map_err(|e| LicenseError::InvalidKey(format!("hex decode: {e}")))?;
        if bytes.len() != 32 {
            return Err(LicenseError::InvalidKey(format!(
                "expected 32-byte secret, got {}",
                bytes.len()
            )));
        }
        let arr: [u8; 32] = bytes
            .try_into()
            .expect("length guarded by preceding check");
        Ok(Self {
            signing_key: SigningKey::from_bytes(&arr),
        })
    }

    /// Issue a license for the given tier and user id, valid for
    /// `valid_days` from now (subscription expiry + 3 day grace typical)
    pub fn issue(
        &self,
        tier: &str,
        user_id: &str,
        valid_days: i64,
    ) -> Result<LicenseKey, LicenseError> {
        let now = Utc::now();
        let payload = LicensePayload {
            tier: tier.to_string(),
            user_id: user_id.to_string(),
            issued_at: now,
            expires_at: now + Duration::days(valid_days),
        };
        let payload_bytes = serde_json::to_vec(&payload).map_err(LicenseError::Serialize)?;
        let signature = self.signing_key.sign(&payload_bytes);
        Ok(LicenseKey {
            payload,
            signature: signature.to_bytes().to_vec(),
        })
    }

    /// Issue a license with an explicit `expires_at` (used when the
    /// Stripe subscription end date is already known)
    pub fn issue_until(
        &self,
        tier: &str,
        user_id: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<LicenseKey, LicenseError> {
        let payload = LicensePayload {
            tier: tier.to_string(),
            user_id: user_id.to_string(),
            issued_at: Utc::now(),
            expires_at,
        };
        let payload_bytes = serde_json::to_vec(&payload).map_err(LicenseError::Serialize)?;
        let signature = self.signing_key.sign(&payload_bytes);
        Ok(LicenseKey {
            payload,
            signature: signature.to_bytes().to_vec(),
        })
    }
}

#[derive(Debug)]
pub enum LicenseError {
    InvalidKey(String),
    Serialize(serde_json::Error),
}

impl std::fmt::Display for LicenseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidKey(msg) => write!(f, "invalid signing key: {msg}"),
            Self::Serialize(err) => write!(f, "serialize error: {err}"),
        }
    }
}

impl std::error::Error for LicenseError {}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Verifier, VerifyingKey};

    // Deterministic 32-byte test secret so tests are reproducible
    const TEST_SECRET_HEX: &str =
        "0000000000000000000000000000000000000000000000000000000000000001";

    #[test]
    fn issue_and_verify_roundtrip() {
        let issuer = LicenseIssuer::from_hex(TEST_SECRET_HEX).unwrap();
        let key = issuer.issue("Pro", "customer_test_1", 33).unwrap();
        assert_eq!(key.payload.tier, "Pro");
        assert_eq!(key.payload.user_id, "customer_test_1");
        assert_eq!(key.signature.len(), 64);

        // Verify with a client-side-style verifier
        let verifying_key = issuer.signing_key.verifying_key();
        let payload_bytes = serde_json::to_vec(&key.payload).unwrap();
        let sig_arr: [u8; 64] = key.signature.as_slice().try_into().unwrap();
        let signature = ed25519_dalek::Signature::from_bytes(&sig_arr);
        verifying_key.verify(&payload_bytes, &signature).unwrap();
    }

    #[test]
    fn base64_roundtrip_matches_core_format() {
        let issuer = LicenseIssuer::from_hex(TEST_SECRET_HEX).unwrap();
        let key = issuer.issue("Pro", "u1", 30).unwrap();
        let encoded = key.to_base64().unwrap();
        // Decode manually and check structure matches
        let bytes = base64::engine::general_purpose::STANDARD.decode(&encoded).unwrap();
        let decoded: LicenseKey = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded.payload.tier, "Pro");
        assert_eq!(decoded.payload.user_id, "u1");
        assert_eq!(decoded.signature, key.signature);
    }

    #[test]
    fn tampered_signature_fails_verify() {
        let issuer = LicenseIssuer::from_hex(TEST_SECRET_HEX).unwrap();
        let mut key = issuer.issue("Pro", "u1", 30).unwrap();
        key.signature[0] ^= 0x01;
        let verifying_key = issuer.signing_key.verifying_key();
        let payload_bytes = serde_json::to_vec(&key.payload).unwrap();
        let sig_arr: [u8; 64] = key.signature.as_slice().try_into().unwrap();
        let signature = ed25519_dalek::Signature::from_bytes(&sig_arr);
        assert!(verifying_key.verify(&payload_bytes, &signature).is_err());
    }

    #[test]
    fn from_hex_rejects_wrong_length() {
        assert!(LicenseIssuer::from_hex("00").is_err());
        assert!(LicenseIssuer::from_hex("").is_err());
    }

    #[test]
    fn from_hex_rejects_bad_encoding() {
        assert!(LicenseIssuer::from_hex("not-hex-at-all-here-obviously-nope").is_err());
    }

    #[test]
    fn issue_until_uses_explicit_expiry() {
        let issuer = LicenseIssuer::from_hex(TEST_SECRET_HEX).unwrap();
        let expiry = Utc::now() + Duration::days(365);
        let key = issuer.issue_until("Pro", "u1", expiry).unwrap();
        // Allow 1 second tolerance for clock movement between issue and assertion
        let diff = (key.payload.expires_at - expiry).num_seconds().abs();
        assert!(diff <= 1, "expires_at should match issue_until arg (diff {diff}s)");
    }

    /// Wire-format contract with `text_to_print_core::license::LicenseKey`:
    /// serialized JSON must contain exactly these top-level fields
    #[test]
    fn wire_format_frozen() {
        let issuer = LicenseIssuer::from_hex(TEST_SECRET_HEX).unwrap();
        let key = issuer.issue("Pro", "u1", 30).unwrap();
        let json = serde_json::to_value(&key).unwrap();
        let obj = json.as_object().unwrap();
        assert!(obj.contains_key("payload"));
        assert!(obj.contains_key("signature"));
        let payload = obj["payload"].as_object().unwrap();
        assert!(payload.contains_key("tier"));
        assert!(payload.contains_key("user_id"));
        assert!(payload.contains_key("issued_at"));
        assert!(payload.contains_key("expires_at"));
    }

    /// Verify that a `LicensePubKey` returned by `LicenseIssuer::verifying_key_bytes`
    /// would work with the client's `LicenseVerifier::new(&[u8; 32])` API
    #[test]
    fn verifying_key_is_32_bytes() {
        let issuer = LicenseIssuer::from_hex(TEST_SECRET_HEX).unwrap();
        let vk_bytes = issuer.signing_key.verifying_key().to_bytes();
        assert_eq!(vk_bytes.len(), 32);
        let vk = VerifyingKey::from_bytes(&vk_bytes).unwrap();
        assert_eq!(vk.to_bytes(), vk_bytes);
    }
}
