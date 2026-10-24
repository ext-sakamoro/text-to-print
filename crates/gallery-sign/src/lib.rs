//! The messages a Gallery author signs, and their verification
//!
//! A message is the canonical `alice-serial` encoding of an array: a fixed domain
//! string, then every field of the request in a fixed order An absent nickname is
//! `Null`, so a signature over one nickname, or over none, does not cover another
//! Each field is length-prefixed by the encoding, so bytes can not move from one
//! field to the next Publish and delete have different domains, so a signature for
//! one is never accepted for the other
//!
//! Verification is Ed25519 with `verify_strict` (it refuses non-canonical scalars and
//! small-order keys)

use alice_serial::{SerialError, Value, encode};
use alice_sign::{KeyPair, PublicKey, Signature};

/// The domain of a publish message
pub const PUBLISH_DOMAIN: &str = "text-to-print gallery publish v1";

/// The domain of a delete message
pub const DELETE_DOMAIN: &str = "text-to-print gallery delete v1";

/// The fields of a `POST /publish` request that its signature covers: all of them
#[derive(Debug, Clone, Copy)]
pub struct PublishFields<'a> {
    pub id: &'a str,
    pub author_did: &'a str,
    pub author_nickname: Option<&'a str>,
    pub lol_source: &'a str,
    pub created_at: &'a str,
}

/// The bytes the author signs for a publish request
///
/// # Errors
/// [`SerialError`] if a field is longer than the encoding allows
pub fn publish_message(fields: &PublishFields<'_>) -> Result<Vec<u8>, SerialError> {
    encode(&Value::Array(vec![
        Value::Str(PUBLISH_DOMAIN.to_owned()),
        Value::Str(fields.id.to_owned()),
        Value::Str(fields.author_did.to_owned()),
        fields
            .author_nickname
            .map_or(Value::Null, |n| Value::Str(n.to_owned())),
        Value::Str(fields.lol_source.to_owned()),
        Value::Str(fields.created_at.to_owned()),
    ]))
}

/// The bytes the author signs for a `DELETE /{id}` request
///
/// # Errors
/// [`SerialError`] if a field is longer than the encoding allows
pub fn delete_message(id: &str, author_did: &str) -> Result<Vec<u8>, SerialError> {
    encode(&Value::Array(vec![
        Value::Str(DELETE_DOMAIN.to_owned()),
        Value::Str(id.to_owned()),
        Value::Str(author_did.to_owned()),
    ]))
}

/// Sign `message` with the Ed25519 key whose 32-byte seed is `seed`
#[must_use]
pub fn sign(seed: [u8; 32], message: &[u8]) -> [u8; 64] {
    KeyPair::from_seed(seed).sign(message).0
}

/// Whether `signature` is a valid strict Ed25519 signature of `message` by `public_key`
#[must_use]
pub fn verify(public_key: [u8; 32], message: &[u8], signature: [u8; 64]) -> bool {
    PublicKey(public_key).verify_strict(message, &Signature(signature))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: [u8; 32] = [11u8; 32];

    fn fields(nickname: Option<&'static str>) -> PublishFields<'static> {
        PublishFields {
            id: "018f4e8c-0000-7000-8000-000000000001",
            author_did: "did:key:aa",
            author_nickname: nickname,
            lol_source: "cube(10)",
            created_at: "2026-10-24T00:00:00Z",
        }
    }

    fn public() -> [u8; 32] {
        KeyPair::from_seed(SEED).public().0
    }

    #[test]
    fn a_signed_publish_verifies() {
        let m = publish_message(&fields(Some("maker"))).unwrap();
        assert!(verify(public(), &m, sign(SEED, &m)));
    }

    #[test]
    fn every_field_changes_the_message() {
        let base = publish_message(&fields(Some("maker"))).unwrap();
        let changed = [
            PublishFields {
                id: "018f4e8c-0000-7000-8000-000000000002",
                ..fields(Some("maker"))
            },
            PublishFields {
                author_did: "did:key:ab",
                ..fields(Some("maker"))
            },
            fields(Some("makers")),
            fields(None),
            PublishFields {
                lol_source: "cube(11)",
                ..fields(Some("maker"))
            },
            PublishFields {
                created_at: "2026-10-24T00:00:01Z",
                ..fields(Some("maker"))
            },
        ];
        for f in changed {
            assert_ne!(publish_message(&f).unwrap(), base, "{f:?}");
        }
    }

    #[test]
    fn bytes_can_not_move_between_fields() {
        let a = publish_message(&PublishFields {
            lol_source: "cube(10)|2026-01-01T",
            created_at: "2026-10-24T00:00:00Z",
            ..fields(None)
        })
        .unwrap();
        let b = publish_message(&PublishFields {
            lol_source: "cube(10)",
            created_at: "2026-01-01T|2026-10-24T00:00:00Z",
            ..fields(None)
        })
        .unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn an_empty_nickname_is_not_an_absent_one() {
        assert_ne!(
            publish_message(&fields(Some(""))).unwrap(),
            publish_message(&fields(None)).unwrap()
        );
    }

    #[test]
    fn publish_and_delete_messages_never_coincide() {
        let d = delete_message("018f4e8c-0000-7000-8000-000000000001", "did:key:aa").unwrap();
        let m = publish_message(&fields(None)).unwrap();
        assert_ne!(d, m);
        assert!(!verify(public(), &d, sign(SEED, &m)));
    }

    #[test]
    fn a_small_order_key_is_refused() {
        // the identity point: any signature with R = identity and s = 0 verifies under
        // the lenient check for every message
        let mut identity = [0u8; 32];
        identity[0] = 1;
        let mut sig = [0u8; 64];
        sig[0] = 1;
        assert!(!verify(identity, b"anything", sig));
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// A change to the encoding, the domain or the field order breaks every signature
    /// already made, so the exact bytes are pinned
    #[test]
    fn the_message_bytes_are_fixed() {
        assert_eq!(
            hex(&publish_message(&fields(Some("maker"))).unwrap()),
            "414c53520100dd00000006\
             db00000020746578742d746f2d7072696e742067616c6c657279207075626c697368207631\
             db0000002430313866346538632d303030302d373030302d383030302d303030303030303030303031\
             db0000000a6469643a6b65793a6161\
             db000000056d616b6572\
             db000000086375626528313029\
             db00000014323032362d31302d32345430303a30303a30305a"
        );
        assert_eq!(
            hex(&delete_message("id1", "did:key:aa").unwrap()),
            "414c53520100dd00000003\
             db0000001f746578742d746f2d7072696e742067616c6c6572792064656c657465207631\
             db00000003696431\
             db0000000a6469643a6b65793a6161"
        );
    }
}
