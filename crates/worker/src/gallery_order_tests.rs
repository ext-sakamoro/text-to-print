//! A gallery post can not choose its place in the listing
//!
//! The author signs `created_at`, but the listing is ordered by the time the worker
//! received the post, so a post dated in the future does not stay on top. `created_at`
//! is accepted only as a UTC time (`Z` or `+00:00`) at most 10 minutes ahead of the
//! worker's clock, and the hourly limits are keyed by the author and the client
//! address, not by the post id the client chooses

use super::*;
use crate::gallery_handler::{delete_buckets, publish_buckets, LIST_SQL};
use ed25519_dalek::SigningKey;

const ID: &str = "018f4e8c-0000-7000-8000-000000000001";
/// 2026-10-25T00:00:00Z
const NOW_MS: u64 = 1_792_886_400_000;

fn signed(created_at: &str) -> (String, String) {
    let key = SigningKey::from_bytes(&[5u8; 32]);
    let did = format!("did:key:{}", hex::encode(key.verifying_key().as_bytes()));
    let unsigned = GalleryPublishFields {
        id: ID,
        author_did: &did,
        author_nickname: None,
        lol_source: "cube(10)",
        created_at,
        signature: "",
    };
    let message = gallery_publish_message(&unsigned).unwrap();
    let sig = hex::encode(text_to_print_gallery_sign::sign(key.to_bytes(), &message));
    (did, sig)
}

fn check(created_at: &str) -> Result<(), RejectReason> {
    let (did, sig) = signed(created_at);
    check_gallery_publish(
        &GalleryPublishFields {
            id: ID,
            author_did: &did,
            author_nickname: None,
            lol_source: "cube(10)",
            created_at,
            signature: &sig,
        },
        NOW_MS,
    )
}

#[test]
fn a_post_dated_far_in_the_future_is_refused() {
    assert!(matches!(
        check("9999-12-31T23:59:59Z"),
        Err(RejectReason::GalleryCreatedAtFuture)
    ));
}

#[test]
fn ten_minutes_ahead_is_the_limit() {
    assert!(check("2026-10-25T00:10:00Z").is_ok());
    assert!(matches!(
        check("2026-10-25T00:10:00.001Z"),
        Err(RejectReason::GalleryCreatedAtFuture)
    ));
}

#[test]
fn past_times_and_both_utc_spellings_are_accepted() {
    assert!(check("2026-10-24T23:59:59Z").is_ok());
    assert!(check("2020-01-01T00:00:00.5Z").is_ok());
    // what chrono's to_rfc3339 sends from released clients
    assert!(check("2026-10-24T23:59:59.123456789+00:00").is_ok());
}

#[test]
fn a_non_utc_offset_is_refused() {
    // +09:00 would sort as text before the same instant written in UTC
    for ts in ["2026-10-25T09:00:00+09:00", "2026-10-24T15:00:00-09:00", "2026-10-24T00:00:00-00:00"] {
        assert!(matches!(check(ts), Err(RejectReason::GalleryCreatedAtFormat)), "{ts}");
    }
}

#[test]
fn the_listing_is_ordered_by_the_time_the_worker_received_each_post() {
    let db = rusqlite::Connection::open_in_memory().unwrap();
    db.execute_batch(include_str!("../migrations/0003_gallery.sql")).unwrap();
    db.execute_batch(include_str!("../migrations/0004_gallery_received_at.sql")).unwrap();
    // (id, created_at the author claims, received_at the worker stored)
    let rows = [
        ("a", "9999-12-31T23:59:59Z", "2026-10-25 00:00:01"),
        ("b", "2026-10-25T00:00:05Z", "2026-10-25 00:00:05"),
        ("c", "2020-01-01T00:00:00Z", "2026-10-25 00:00:03"),
    ];
    for (id, created, received) in rows {
        db.execute(
            "INSERT INTO gallery_sdfs (id, author_did, lol_source, created_at, signature, received_at)
             VALUES (?1, 'did:key:x', 'cube(1)', ?2, 's', ?3)",
            rusqlite::params![id, created, received],
        )
        .unwrap();
    }
    let mut stmt = db.prepare(LIST_SQL).unwrap();
    let ids: Vec<String> = stmt
        .query_map(rusqlite::params![10, 0], |r| r.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(ids, ["b", "c", "a"]);
}

#[test]
fn publish_limits_are_per_author_and_per_address_not_per_post_id() {
    let keys = publish_buckets("did:key:aa", "iphash");
    assert!(keys.iter().all(|k| !k.contains(ID)), "{keys:?}");
    assert!(keys.contains(&"gallery_did:did:key:aa".to_string()), "{keys:?}");
    assert!(keys.contains(&"gallery_ip:iphash".to_string()), "{keys:?}");
}

#[test]
fn deletes_share_the_address_limit() {
    assert_eq!(delete_buckets("iphash"), ["gallery_ip:iphash".to_string()]);
}
