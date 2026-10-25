use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use ed25519_dalek::{PUBLIC_KEY_LENGTH, SIGNATURE_LENGTH, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::tier::Tier;

#[derive(Debug, Serialize, Deserialize)]
pub struct LicensePayload {
    pub tier: Tier,
    pub user_id: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LicenseKey {
    pub payload: LicensePayload,
    pub signature: Vec<u8>,
    /// The payload bytes the signature covers, exactly as issued or as received
    /// (empty for a key built by hand, which then verifies against nothing)
    #[serde(skip)]
    pub signed_payload: Vec<u8>,
}

/// The wire form of a key: the payload is kept as the bytes it arrived as
#[derive(Serialize, Deserialize)]
struct LicenseKeyWire<'a> {
    #[serde(borrow)]
    payload: &'a serde_json::value::RawValue,
    signature: Vec<u8>,
}

/// ライセンス発行者（サーバー側 / CLI で使用）
pub struct LicenseIssuer {
    signing_key: SigningKey,
}

impl LicenseIssuer {
    /// 新しい鍵ペアを生成
    pub fn generate() -> Self {
        let mut rng = rand::thread_rng();
        Self {
            signing_key: SigningKey::generate(&mut rng),
        }
    }

    /// 秘密鍵バイトから復元
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        Self {
            signing_key: SigningKey::from_bytes(bytes),
        }
    }

    /// 秘密鍵バイト
    pub fn secret_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }

    /// 公開鍵バイト（バイナリに埋め込み用）
    pub fn public_key_bytes(&self) -> [u8; PUBLIC_KEY_LENGTH] {
        self.signing_key.verifying_key().to_bytes()
    }

    /// ライセンスキーを発行
    pub fn issue(&self, tier: Tier, user_id: &str, valid_days: i64) -> Result<LicenseKey> {
        let issued_at = Utc::now();
        // `Duration::days` and `DateTime + Duration` panic for a span chrono cannot represent
        // (`i64::MAX` days, or just 100 million days): report it as an error instead
        let expires_at = chrono::Duration::try_days(valid_days)
            .and_then(|span| issued_at.checked_add_signed(span))
            .with_context(|| {
                format!("valid_days = {valid_days} is outside the representable range (about +-96 million days)")
            })?;
        let payload = LicensePayload {
            tier,
            user_id: user_id.to_string(),
            issued_at,
            expires_at,
        };

        let payload_bytes = serde_json::to_vec(&payload)?;
        let signature = self.signing_key.sign(&payload_bytes);

        Ok(LicenseKey {
            payload,
            signature: signature.to_bytes().to_vec(),
            signed_payload: payload_bytes,
        })
    }
}

/// ライセンス検証器（アプリ側で使用）
pub struct LicenseVerifier {
    public_key: [u8; PUBLIC_KEY_LENGTH],
}

impl LicenseVerifier {
    pub fn new(public_key_bytes: &[u8; PUBLIC_KEY_LENGTH]) -> Result<Self> {
        // refuse bytes that are not a curve point here rather than at every verify
        VerifyingKey::from_bytes(public_key_bytes)?;
        Ok(Self {
            public_key: *public_key_bytes,
        })
    }

    /// DID の公開鍵からライセンス検証器を作成
    pub fn from_did_public_key(public_key: &[u8]) -> Result<Self> {
        if public_key.len() != PUBLIC_KEY_LENGTH {
            bail!("invalid public key length: {}", public_key.len());
        }
        let bytes: [u8; PUBLIC_KEY_LENGTH] = public_key
            .try_into()
            .expect("length guarded by preceding check");
        Self::new(&bytes)
    }

    /// [`Self::verify_at`] with the system clock
    pub fn verify(&self, key: &LicenseKey) -> Result<LicensePayload> {
        self.verify_at(key, Utc::now())
    }

    /// The payload read from the signed bytes, if the signature is a strict Ed25519
    /// signature (alice-sign `verify_strict`) of the payload bytes exactly as issued
    /// or received, and `now` is not after its `expires_at` (`key.payload` is not
    /// trusted: a changed field there is not what was signed)
    pub fn verify_at(&self, key: &LicenseKey, now: DateTime<Utc>) -> Result<LicensePayload> {
        let sig_bytes: [u8; SIGNATURE_LENGTH] = key
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid signature length"))?;
        if !alice_sign::PublicKey(self.public_key)
            .verify_strict(&key.signed_payload, &alice_sign::Signature(sig_bytes))
        {
            bail!("license signature does not verify");
        }
        let payload: LicensePayload = serde_json::from_slice(&key.signed_payload)?;
        if now > payload.expires_at {
            bail!("license expired");
        }
        Ok(payload)
    }
}

impl LicenseKey {
    /// The base64 of `{"payload": <the signed payload bytes>, "signature": [...]}`
    pub fn to_base64(&self) -> Result<String> {
        use base64::Engine;
        let payload = std::str::from_utf8(&self.signed_payload)
            .ok()
            .and_then(|s| serde_json::value::RawValue::from_string(s.to_owned()).ok())
            .context("the key has no signed payload bytes")?;
        let bytes = serde_json::to_vec(&LicenseKeyWire {
            payload: &payload,
            signature: self.signature.clone(),
        })?;
        Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
    }

    /// Decode a key, keeping the payload bytes it carries for [`LicenseVerifier::verify_at`]
    pub fn from_base64(encoded: &str) -> Result<Self> {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD.decode(encoded.trim())?;
        let wire: LicenseKeyWire<'_> = serde_json::from_slice(&bytes)?;
        let payload: LicensePayload = serde_json::from_str(wire.payload.get())?;
        Ok(Self {
            payload,
            signature: wire.signature,
            signed_payload: wire.payload.get().as_bytes().to_vec(),
        })
    }
}

/// The tier a stored key (base64, as saved with the profile) gives at `now`: the
/// payload's tier if the key verifies and has not expired, otherwise Free
#[must_use]
pub fn tier_of_stored_key(stored: &str, verifier: &LicenseVerifier, now: DateTime<Utc>) -> Tier {
    if stored.trim().is_empty() {
        return Tier::Free;
    }
    LicenseKey::from_base64(stored)
        .ok()
        .and_then(|key| verifier.verify_at(&key, now).ok().map(|p| p.tier))
        .unwrap_or(Tier::Free)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_and_verify_roundtrip() {
        let issuer = LicenseIssuer::generate();
        let key = issuer.issue(Tier::Pro, "user-42", 30).unwrap();

        let verifier = LicenseVerifier::new(&issuer.public_key_bytes()).unwrap();
        let payload = verifier.verify(&key).unwrap();

        assert_eq!(payload.tier, Tier::Pro);
        assert_eq!(payload.user_id, "user-42");
    }

    #[test]
    fn base64_roundtrip() {
        let issuer = LicenseIssuer::generate();
        let key = issuer.issue(Tier::General, "user-1", 365).unwrap();

        let encoded = key.to_base64().unwrap();
        let decoded = LicenseKey::from_base64(&encoded).unwrap();

        let verifier = LicenseVerifier::new(&issuer.public_key_bytes()).unwrap();
        let payload = verifier.verify(&decoded).unwrap();
        assert_eq!(payload.tier, Tier::General);
    }

    #[test]
    fn wrong_public_key_rejects() {
        let issuer = LicenseIssuer::generate();
        let key = issuer.issue(Tier::Pro, "user", 30).unwrap();

        let other_issuer = LicenseIssuer::generate();
        let verifier = LicenseVerifier::new(&other_issuer.public_key_bytes()).unwrap();
        assert!(verifier.verify(&key).is_err());
    }

    #[test]
    fn tampered_payload_rejects() {
        let issuer = LicenseIssuer::generate();
        let verifier = LicenseVerifier::new(&issuer.public_key_bytes()).unwrap();
        // a field changed in memory is not what was signed: the signed tier is returned
        let mut key = issuer.issue(Tier::Free, "user", 30).unwrap();
        key.payload.tier = Tier::Enterprise;
        assert_eq!(verifier.verify(&key).unwrap().tier, Tier::Free);
        // a field changed in the encoded key is refused
        use base64::Engine;
        let json = String::from_utf8(
            base64::engine::general_purpose::STANDARD
                .decode(
                    issuer
                        .issue(Tier::Free, "user", 30)
                        .unwrap()
                        .to_base64()
                        .unwrap(),
                )
                .unwrap(),
        )
        .unwrap();
        assert!(json.contains("\"tier\":\"Free\""));
        let forged = base64::engine::general_purpose::STANDARD
            .encode(json.replace("\"tier\":\"Free\"", "\"tier\":\"Enterprise\""));
        assert!(
            verifier
                .verify(&LicenseKey::from_base64(&forged).unwrap())
                .is_err()
        );
    }

    #[test]
    fn expired_license_rejects() {
        let issuer = LicenseIssuer::generate();
        let key = issuer.issue(Tier::Pro, "user", -1).unwrap();

        let verifier = LicenseVerifier::new(&issuer.public_key_bytes()).unwrap();
        assert!(verifier.verify(&key).is_err());
    }

    #[test]
    fn invalid_signature_length_rejected() {
        let key = LicenseKey {
            payload: LicensePayload {
                tier: Tier::Pro,
                user_id: "test".to_string(),
                issued_at: Utc::now(),
                expires_at: Utc::now() + chrono::Duration::days(30),
            },
            signature: vec![0u8; 10],
            signed_payload: Vec::new(),
        };
        let issuer = LicenseIssuer::generate();
        let verifier = LicenseVerifier::new(&issuer.public_key_bytes()).unwrap();
        assert!(verifier.verify(&key).is_err());
    }

    #[test]
    fn from_base64_invalid_input() {
        assert!(LicenseKey::from_base64("not-valid-base64!!!").is_err());
    }

    #[test]
    fn all_tiers_can_be_issued() {
        let issuer = LicenseIssuer::generate();
        let verifier = LicenseVerifier::new(&issuer.public_key_bytes()).unwrap();

        for tier in [Tier::Free, Tier::General, Tier::Pro, Tier::Enterprise] {
            let key = issuer.issue(tier, "user", 30).unwrap();
            let payload = verifier.verify(&key).unwrap();
            assert_eq!(payload.tier, tier);
        }
    }

    #[test]
    fn issuer_from_bytes_roundtrip() {
        let issuer = LicenseIssuer::generate();
        let secret = issuer.signing_key.to_bytes();
        let restored = LicenseIssuer::from_bytes(&secret);
        assert_eq!(issuer.public_key_bytes(), restored.public_key_bytes());
    }
}
