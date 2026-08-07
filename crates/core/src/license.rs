use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use ed25519_dalek::{
    PUBLIC_KEY_LENGTH, SIGNATURE_LENGTH, Signature, Signer, SigningKey, Verifier, VerifyingKey,
};
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
        let payload = LicensePayload {
            tier,
            user_id: user_id.to_string(),
            issued_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::days(valid_days),
        };

        let payload_bytes = serde_json::to_vec(&payload)?;
        let signature = self.signing_key.sign(&payload_bytes);

        Ok(LicenseKey {
            payload,
            signature: signature.to_bytes().to_vec(),
        })
    }
}

/// ライセンス検証器（アプリ側で使用）
pub struct LicenseVerifier {
    public_key: VerifyingKey,
}

impl LicenseVerifier {
    pub fn new(public_key_bytes: &[u8; PUBLIC_KEY_LENGTH]) -> Result<Self> {
        Ok(Self {
            public_key: VerifyingKey::from_bytes(public_key_bytes)?,
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

    pub fn verify<'k>(&self, key: &'k LicenseKey) -> Result<&'k LicensePayload> {
        let payload_bytes = serde_json::to_vec(&key.payload)?;

        if key.signature.len() != SIGNATURE_LENGTH {
            bail!("invalid signature length");
        }
        let sig_bytes: [u8; SIGNATURE_LENGTH] = key.signature[..SIGNATURE_LENGTH].try_into()?;
        let signature = Signature::from_bytes(&sig_bytes);

        self.public_key.verify(&payload_bytes, &signature)?;

        if Utc::now() > key.payload.expires_at {
            bail!("license expired");
        }

        Ok(&key.payload)
    }
}

impl LicenseKey {
    pub fn to_base64(&self) -> Result<String> {
        use base64::Engine;
        let bytes = serde_json::to_vec(self)?;
        Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
    }

    pub fn from_base64(encoded: &str) -> Result<Self> {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD.decode(encoded.trim())?;
        let key: Self = serde_json::from_slice(&bytes)?;
        Ok(key)
    }
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
        let mut key = issuer.issue(Tier::Free, "user", 30).unwrap();
        key.payload.tier = Tier::Enterprise;

        let verifier = LicenseVerifier::new(&issuer.public_key_bytes()).unwrap();
        assert!(verifier.verify(&key).is_err());
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
