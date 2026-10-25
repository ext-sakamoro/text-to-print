//! A gallery publish is accepted only for the fields its author signed
//!
//! Each test signs a publish request the way the desktop client does
//! (`gallery_client::publish`) and then changes what a relay on the way
//! could change. The request must be refused: a valid signature over one
//! request must not make a different request acceptable

use super::*;
use ed25519_dalek::SigningKey;

const ID: &str = "018f4e8c-0000-7000-8000-000000000001";

/// A worker clock after every time these tests sign (2027-01-01T00:00:00Z)
const TEST_NOW_MS: u64 = 1_798_761_600_000;

fn check(f: &GalleryPublishFields<'_>) -> Result<(), RejectReason> {
    check_gallery_publish(f, TEST_NOW_MS)
}

fn identity() -> (SigningKey, String) {
    let key = SigningKey::from_bytes(&[11u8; 32]);
    let did = format!("did:key:{}", hex::encode(key.verifying_key().as_bytes()));
    (key, did)
}

fn sign(
    key: &SigningKey,
    did: &str,
    nickname: Option<&str>,
    lol: &str,
    created_at: &str,
) -> String {
    let message = gallery_publish_message(&fields(did, nickname, lol, created_at, "")).unwrap();
    hex::encode(text_to_print_gallery_sign::sign(key.to_bytes(), &message))
}

fn fields<'a>(
    did: &'a str,
    nickname: Option<&'a str>,
    lol: &'a str,
    created_at: &'a str,
    signature: &'a str,
) -> GalleryPublishFields<'a> {
    GalleryPublishFields {
        id: ID,
        author_did: did,
        author_nickname: nickname,
        lol_source: lol,
        created_at,
        signature,
    }
}

#[test]
fn the_signed_request_is_accepted() {
    let (key, did) = identity();
    let created_at = "2026-10-24T00:00:00Z";
    let sig = sign(&key, &did, Some("maker"), "cube(10)", created_at);
    let r = check(&fields(&did, Some("maker"), "cube(10)", created_at, &sig));
    assert!(r.is_ok(), "the signed request was refused: {r:?}");
}

/// The nickname is shown next to the post as its author's name; a relay that
/// keeps the signature and replaces the nickname must not be accepted
#[test]
fn a_changed_nickname_is_refused() {
    let (key, did) = identity();
    let created_at = "2026-10-24T00:00:00Z";
    let sig = sign(&key, &did, Some("maker"), "cube(10)", created_at);
    let r = check(&fields(&did, Some("maker"), "cube(10)", created_at, &sig));
    assert!(r.is_ok(), "the signed request was refused: {r:?}");
    assert!(
        check(&fields(
            &did,
            Some("someone else"),
            "cube(10)",
            created_at,
            &sig
        ))
        .is_err(),
        "the same signature was accepted with a different nickname"
    );
    assert!(
        check(&fields(&did, None, "cube(10)", created_at, &sig)).is_err(),
        "the same signature was accepted with the nickname removed"
    );
}

/// The signed message joins the fields with `|`, and `lol_source` may hold
/// `|`; the author signed `lol_source = "cube(10)|2026-01-01T"`, and a relay
/// moves its tail into `created_at`, which still passes the timestamp check
/// The stored source and date are then ones the author never signed
#[test]
fn bytes_moved_between_the_source_and_the_date_are_refused() {
    let (key, did) = identity();
    let signed_lol = "cube(10)|2026-01-01T";
    let signed_created_at = "2026-10-24T00:00:00Z";
    let sig = sign(&key, &did, None, signed_lol, signed_created_at);
    let r = check(&fields(&did, None, signed_lol, signed_created_at, &sig));
    assert!(r.is_ok(), "the signed request was refused: {r:?}");
    let forged_created_at = format!("2026-01-01T|{signed_created_at}");
    assert!(
        check(&fields(&did, None, "cube(10)", &forged_created_at, &sig)).is_err(),
        "a request with a different lol_source ({:?}) and created_at ({forged_created_at:?}) \
         was accepted under the signature of {signed_lol:?}",
        "cube(10)"
    );
}
