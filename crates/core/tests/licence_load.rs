//! The tier the app runs with comes from the stored licence key, checked again each
//! time it is loaded: the signature over the payload bytes exactly as received, and
//! the expiry against the clock the caller passes. An invalid or expired key is Free,
//! whatever tier the profile row says

use base64::Engine;
use chrono::Duration;
use text_to_print_core::db::Database;
use text_to_print_core::license::{LicenseIssuer, LicenseKey, LicenseVerifier, tier_of_stored_key};
use text_to_print_core::tier::Tier;

fn issuer() -> LicenseIssuer {
    LicenseIssuer::from_bytes(&[3u8; 32])
}

fn verifier() -> LicenseVerifier {
    LicenseVerifier::new(&issuer().public_key_bytes()).unwrap()
}

#[test]
fn a_key_counts_until_it_expires_and_not_after() {
    let key = issuer().issue(Tier::Pro, "user", 30).unwrap();
    let stored = key.to_base64().unwrap();
    let expires = key.payload.expires_at;
    let v = verifier();
    assert_eq!(
        tier_of_stored_key(&stored, &v, expires - Duration::seconds(1)),
        Tier::Pro
    );
    assert_eq!(
        tier_of_stored_key(&stored, &v, expires + Duration::seconds(1)),
        Tier::Free
    );
    assert!(v.verify_at(&key, expires - Duration::seconds(1)).is_ok());
    assert!(v.verify_at(&key, expires + Duration::seconds(1)).is_err());
}

#[test]
fn no_key_garbage_or_another_issuer_is_free() {
    let v = verifier();
    let now = chrono::Utc::now();
    assert_eq!(tier_of_stored_key("", &v, now), Tier::Free);
    assert_eq!(tier_of_stored_key("not a key", &v, now), Tier::Free);
    let other = LicenseIssuer::from_bytes(&[4u8; 32])
        .issue(Tier::Enterprise, "u", 30)
        .unwrap();
    assert_eq!(
        tier_of_stored_key(&other.to_base64().unwrap(), &v, now),
        Tier::Free
    );
}

#[test]
fn the_signature_covers_the_payload_bytes_as_received() {
    // the same payload written with different bytes (pretty-printed) keeps its
    // signature only if the verifier re-serialises; checked over the received
    // bytes, it is refused
    let key = issuer().issue(Tier::Pro, "user", 30).unwrap();
    let raw: serde_json::Value = serde_json::from_slice(
        &base64::engine::general_purpose::STANDARD
            .decode(key.to_base64().unwrap())
            .unwrap(),
    )
    .unwrap();
    let reworded = format!(
        "{{\"payload\":{},\"signature\":{}}}",
        serde_json::to_string_pretty(&raw["payload"]).unwrap(),
        raw["signature"]
    );
    let stored = base64::engine::general_purpose::STANDARD.encode(reworded);
    let now = key.payload.issued_at;
    assert_eq!(tier_of_stored_key(&stored, &verifier(), now), Tier::Free);
    // and the key as issued still counts
    assert_eq!(
        tier_of_stored_key(&key.to_base64().unwrap(), &verifier(), now),
        Tier::Pro
    );
}

#[test]
fn a_decoded_key_verifies_like_the_issued_one() {
    let key = issuer().issue(Tier::General, "user", 30).unwrap();
    let decoded = LicenseKey::from_base64(&key.to_base64().unwrap()).unwrap();
    assert!(
        verifier()
            .verify_at(&decoded, key.payload.issued_at)
            .is_ok()
    );
}

#[test]
fn loading_the_profile_drops_an_expired_paid_tier() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    db.ensure_profile("p").unwrap();
    let key = issuer().issue(Tier::Pro, "user", 30).unwrap();
    db.update_profile_tier("p", "Pro", &key.to_base64().unwrap())
        .unwrap();
    let v = verifier();
    let expires = key.payload.expires_at;
    assert_eq!(
        db.load_tier("p", &v, expires - Duration::seconds(1))
            .unwrap(),
        Tier::Pro
    );
    assert_eq!(
        db.load_tier("p", &v, expires + Duration::seconds(1))
            .unwrap(),
        Tier::Free
    );
    // a row that says Pro without a valid key is Free
    db.update_profile_tier("p", "Pro", "").unwrap();
    assert_eq!(
        db.load_tier("p", &v, expires - Duration::seconds(1))
            .unwrap(),
        Tier::Free
    );
}
