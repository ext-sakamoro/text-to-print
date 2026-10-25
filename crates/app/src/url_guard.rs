//! Allow-lists for the only two external URLs the app opens in a browser.
//!
//! The checkout URL comes from the backend's Stripe response and the updater
//! URL from the GitHub releases API; neither is fully trusted, so each is
//! checked against an exact-host (and, for the updater, an exact path prefix)
//! allow-list before `open::that`. `UrlAllowList` rejects userinfo, non-default
//! ports, non-https schemes, `.`/`..` segments, percent-encoded separators and
//! IDN/host lookalikes, so a tampered response cannot open an arbitrary target.

use alice_browser_secure::UrlAllowList;

/// Stripe hosted-checkout pages only (`https://checkout.stripe.com/…`).
///
/// Host-only: Stripe's checkout path is not a documented stable prefix, so the
/// exact host plus the list's built-in refusals are the guard.
pub fn checkout_allow_list() -> UrlAllowList {
    UrlAllowList::new(&["https"], &["checkout.stripe.com"])
        .expect("static checkout allow-list is well-formed")
}

/// This product's own GitHub release pages only
/// (`https://github.com/ext-sakamoro/text-to-print/releases/…`).
///
/// Path-prefixed so a tampered release response cannot open another repository
/// on the same host.
pub fn updater_allow_list() -> UrlAllowList {
    UrlAllowList::new(&["https"], &["github.com"])
        .expect("static updater allow-list is well-formed")
        .with_path_prefix("/ext-sakamoro/text-to-print/releases")
        .expect("static updater path prefix is well-formed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkout_allows_stripe_host_only() {
        let l = checkout_allow_list();
        assert!(l.allows("https://checkout.stripe.com/c/pay/cs_test_abc123"));
        // wrong host, lookalike host, non-https, userinfo, non-default port: all refused
        assert!(!l.allows("https://evil.example.com/pay"));
        assert!(!l.allows("https://checkout.stripe.com.evil.com/c/pay/x"));
        assert!(!l.allows("http://checkout.stripe.com/c/pay/x"));
        assert!(!l.allows("https://user@checkout.stripe.com/c/pay/x"));
        assert!(!l.allows("https://checkout.stripe.com:8443/c/pay/x"));
    }

    #[test]
    fn updater_allows_only_this_repos_releases() {
        let l = updater_allow_list();
        // the real release page (and the bare prefix) are allowed
        assert!(l.allows("https://github.com/ext-sakamoro/text-to-print/releases/tag/v1.2.3"));
        assert!(l.allows("https://github.com/ext-sakamoro/text-to-print/releases"));
        // another repository under the same host
        assert!(!l.allows("https://github.com/attacker/evil/releases/tag/v1"));
        // a path that tries to climb out of the allowed prefix (dot segments refused)
        assert!(
            !l.allows("https://github.com/ext-sakamoro/text-to-print/releases/../../attacker/evil")
        );
        // percent-encoded separators refused
        assert!(
            !l.allows("https://github.com/ext-sakamoro/text-to-print/releases/..%2f..%2fattacker")
        );
        // lookalike host
        assert!(
            !l.allows("https://github.com.evil.com/ext-sakamoro/text-to-print/releases/tag/v1")
        );
        // non-https
        assert!(!l.allows("http://github.com/ext-sakamoro/text-to-print/releases/tag/v1"));
        // a sibling prefix that merely starts with the same text (whole-segment match)
        assert!(!l.allows("https://github.com/ext-sakamoro/text-to-print-evil/releases/tag/v1"));
    }
}
