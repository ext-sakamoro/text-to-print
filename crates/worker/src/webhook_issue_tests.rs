//! A licence is issued once per payment, and only for a payment that went through
//!
//! A checkout session mints a key only when Stripe says it is paid (or needs no
//! payment); an unpaid one waits for `checkout.session.async_payment_succeeded`.
//! A failed handler is not recorded as processed, so Stripe retries it, and the
//! retry finds the key the first attempt stored instead of minting a second one

use super::*;

fn session(status: Option<&str>) -> serde_json::Value {
    let mut s = serde_json::json!({"id": "cs_test_1", "customer": "cus_1"});
    if let Some(st) = status {
        s["payment_status"] = st.into();
    }
    s
}

#[test]
fn only_a_settled_checkout_issues_a_licence() {
    assert_eq!(
        checkout_decision(&session(Some("paid"))),
        CheckoutDecision::Issue
    );
    assert_eq!(
        checkout_decision(&session(Some("no_payment_required"))),
        CheckoutDecision::Issue
    );
    assert_eq!(
        checkout_decision(&session(Some("unpaid"))),
        CheckoutDecision::AwaitPayment
    );
    // a status Stripe does not document, or none, does not issue
    assert_eq!(
        checkout_decision(&session(Some("other"))),
        CheckoutDecision::AwaitPayment
    );
    assert_eq!(
        checkout_decision(&session(None)),
        CheckoutDecision::AwaitPayment
    );
}

#[test]
fn a_failed_handler_is_retried_and_a_good_one_is_recorded() {
    assert_eq!(webhook_outcome(&Ok(())), (true, 200));
    assert_eq!(webhook_outcome(&Err("email".to_string())), (false, 500));
}

#[test]
fn a_retry_after_a_partial_success_resends_the_stored_key() {
    let db = rusqlite::Connection::open_in_memory().unwrap();
    db.execute_batch(include_str!("../migrations/0002_stripe.sql"))
        .unwrap();
    db.execute_batch(include_str!("../migrations/0005_license_issue_key.sql"))
        .unwrap();
    db.execute(
        "INSERT INTO subscribers (stripe_customer_id, email, tier, status, created_at, updated_at)
         VALUES ('cus_1', 'a@example.com', 'Pro', 'active', 't', 't')",
        [],
    )
    .unwrap();
    let issue_key = checkout_issue_key("cs_test_1");
    let attempt = |id: &str, key: &str| -> String {
        db.execute(
            INSERT_LICENSE_SQL,
            rusqlite::params![id, "cus_1", key, "Pro", "i", "e", issue_key],
        )
        .unwrap();
        db.query_row(SELECT_ISSUED_LICENSE_SQL, [&issue_key], |r| r.get(0))
            .unwrap()
    };
    // first attempt stores key A, then the email fails and the handler errors
    assert_eq!(attempt("row-1", "KEY-A"), "KEY-A");
    // Stripe retries: a fresh key B is minted but the stored A is what gets sent
    assert_eq!(attempt("row-2", "KEY-B"), "KEY-A");
    let rows: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM licenses WHERE issue_key = ?1",
            [&issue_key],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rows, 1);
}

#[test]
fn renewals_are_keyed_by_subscription_and_period() {
    assert_eq!(
        subscription_issue_key("sub_1", 1_800_000_000),
        "subscription:sub_1:1800000000"
    );
    assert_ne!(
        subscription_issue_key("sub_1", 1_800_000_000),
        subscription_issue_key("sub_1", 1_802_592_000)
    );
    assert_eq!(checkout_issue_key("cs_test_1"), "checkout:cs_test_1");
}
