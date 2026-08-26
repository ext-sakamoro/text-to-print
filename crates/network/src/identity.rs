use anyhow::Result;
use ed25519_dalek::{SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Did {
    pub id: String,
    pub public_key: Vec<u8>,
}

pub struct Identity {
    signing_key: SigningKey,
    pub did: Did,
}

impl Identity {
    pub fn load_or_create(data_dir: &Path) -> Result<Self> {
        let key_path = data_dir.join("identity.key");

        let signing_key = if key_path.exists() {
            let bytes = std::fs::read(&key_path)?;
            let key_bytes: [u8; 32] = bytes[..32].try_into()?;
            SigningKey::from_bytes(&key_bytes)
        } else {
            let mut rng = rand::thread_rng();
            let key = SigningKey::generate(&mut rng);
            std::fs::create_dir_all(data_dir)?;
            std::fs::write(&key_path, key.to_bytes())?;
            key
        };

        let verifying_key = signing_key.verifying_key();
        let did = Did {
            id: format!("did:key:{}", hex::encode(verifying_key.as_bytes())),
            public_key: verifying_key.as_bytes().to_vec(),
        };

        Ok(Self { signing_key, did })
    }

    pub fn sign(&self, data: &[u8]) -> Vec<u8> {
        use ed25519_dalek::Signer;
        self.signing_key.sign(data).to_bytes().to_vec()
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// Ref to the underlying ed25519 signing key Introduced for
    /// Gallery Phase 3 (2026-08-26) so [`crate::gallery_client`] can
    /// sign publish / delete canonical messages without duplicating
    /// the signing surface Callers must not persist or leak this ref
    #[must_use]
    pub fn signing_key(&self) -> &SigningKey {
        &self.signing_key
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Verifier;

    #[test]
    fn create_new_identity() {
        let dir = tempfile::tempdir().unwrap();
        let id = Identity::load_or_create(dir.path()).unwrap();
        assert!(id.did.id.starts_with("did:key:"));
        assert_eq!(id.did.public_key.len(), 32);
    }

    #[test]
    fn identity_persists_across_loads() {
        let dir = tempfile::tempdir().unwrap();
        let id1 = Identity::load_or_create(dir.path()).unwrap();
        let id2 = Identity::load_or_create(dir.path()).unwrap();
        assert_eq!(id1.did.id, id2.did.id);
        assert_eq!(id1.did.public_key, id2.did.public_key);
    }

    #[test]
    fn sign_and_verify() {
        let dir = tempfile::tempdir().unwrap();
        let id = Identity::load_or_create(dir.path()).unwrap();
        let data = b"hello ALICE";
        let sig_bytes = id.sign(data);
        let sig = ed25519_dalek::Signature::from_bytes(sig_bytes[..64].try_into().unwrap());
        assert!(id.verifying_key().verify(data, &sig).is_ok());
    }

    #[test]
    fn sign_rejects_tampered_data() {
        let dir = tempfile::tempdir().unwrap();
        let id = Identity::load_or_create(dir.path()).unwrap();
        let sig_bytes = id.sign(b"original");
        let sig = ed25519_dalek::Signature::from_bytes(sig_bytes[..64].try_into().unwrap());
        assert!(id.verifying_key().verify(b"tampered", &sig).is_err());
    }

    #[test]
    fn did_format() {
        let dir = tempfile::tempdir().unwrap();
        let id = Identity::load_or_create(dir.path()).unwrap();
        // did:key: + 64 hex chars (32 bytes)
        assert_eq!(id.did.id.len(), "did:key:".len() + 64);
    }
}
