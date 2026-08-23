//! OS Keychain integration for BYO LLM API keys (2026-08-23)
//!
//! Wraps the `keyring` crate to store OpenAI-compat provider API keys
//! in the platform-native secret store:
//! - macOS: Keychain Services
//! - Windows: Credential Manager
//! - Linux: Secret Service (via libsecret)
//!
//! Service name is fixed to [`SERVICE_NAME`]; account names come from
//! [`text_to_print_llm::openai_compat_backend::OpenAiCompatProvider::keychain_account`]
//! so each provider stores its key under a separate slot
//!
//! **Never** persist API keys to on-disk config Only the Keychain
//! backend can be trusted with vendor secrets

use anyhow::{Context, Result};
use keyring::Entry;

/// Keychain service identifier Matches the desktop app's bundle
/// identifier so Keychain UI (macOS `security list-keychain-items`,
/// etc) surfaces entries under the text-to-print app name
pub const SERVICE_NAME: &str = "net.alicelaw.text-to-print";

/// Store an API key for the given provider account Overwrites any
/// existing value under the same account slug
///
/// # Errors
///
/// Backend errors (Keychain locked, Secret Service unavailable, etc)
/// bubble up wrapped with account context
pub fn set_api_key(provider_account: &str, api_key: &str) -> Result<()> {
    let entry = Entry::new(SERVICE_NAME, provider_account)
        .with_context(|| format!("failed to construct Keychain entry for {provider_account}"))?;
    entry
        .set_password(api_key)
        .with_context(|| format!("failed to store API key for {provider_account}"))?;
    Ok(())
}

/// Retrieve the API key for the given provider account Returns
/// `Ok(None)` when no entry exists (never stored, or user deleted it)
///
/// # Errors
///
/// Backend errors other than `NoEntry` are surfaced as `Err`
pub fn get_api_key(provider_account: &str) -> Result<Option<String>> {
    let entry = Entry::new(SERVICE_NAME, provider_account)
        .with_context(|| format!("failed to construct Keychain entry for {provider_account}"))?;
    match entry.get_password() {
        Ok(pw) => Ok(Some(pw)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e).with_context(|| format!("failed to read API key for {provider_account}")),
    }
}

/// Delete the stored API key for the given provider account Idempotent
/// — no-op if there is no entry
///
/// # Errors
///
/// Backend errors other than `NoEntry` are surfaced as `Err`
pub fn delete_api_key(provider_account: &str) -> Result<()> {
    let entry = Entry::new(SERVICE_NAME, provider_account)
        .with_context(|| format!("failed to construct Keychain entry for {provider_account}"))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => {
            Err(e).with_context(|| format!("failed to delete API key for {provider_account}"))
        }
    }
}

/// Sanity check: set + get + delete round-trip under a probe account
/// Used by the Settings UI's "Test Keychain" button to verify the
/// platform backend works before the user pastes a real key
///
/// # Errors
///
/// Any backend error along the round-trip, or a value-mismatch if the
/// backend silently corrupts the value
pub fn probe() -> Result<()> {
    const PROBE_ACCOUNT: &str = "__keychain_probe";
    const PROBE_VALUE: &str = "probe-value";
    set_api_key(PROBE_ACCOUNT, PROBE_VALUE)?;
    let got = get_api_key(PROBE_ACCOUNT)?;
    delete_api_key(PROBE_ACCOUNT)?;
    if got.as_deref() != Some(PROBE_VALUE) {
        anyhow::bail!("Keychain probe round-trip returned unexpected value");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests hit the actual OS Keychain, which:
    // - requires an interactive user session (fails in headless CI)
    // - may prompt for password on some Linux setups
    // - collides with other test runs if executed in parallel
    //
    // Marked `#[ignore]` by default so `cargo test` stays hermetic Run
    // with `cargo test -p text-to-print-core -- --ignored keychain`
    // when touching Keychain code on an interactive machine

    #[test]
    #[ignore = "hits OS Keychain, run explicitly with --ignored"]
    fn probe_roundtrip() {
        probe().expect("Keychain probe must succeed on interactive systems");
    }

    #[test]
    #[ignore = "hits OS Keychain, run explicitly with --ignored"]
    fn set_get_delete_roundtrip() {
        let account = format!("__test_roundtrip_{}", std::process::id());
        set_api_key(&account, "sk-test-value").unwrap();
        let got = get_api_key(&account).unwrap();
        assert_eq!(got.as_deref(), Some("sk-test-value"));
        delete_api_key(&account).unwrap();
        let after_delete = get_api_key(&account).unwrap();
        assert!(after_delete.is_none());
    }

    #[test]
    #[ignore = "hits OS Keychain, run explicitly with --ignored"]
    fn get_missing_entry_returns_none() {
        let account = format!("__test_missing_{}", std::process::id());
        let _ = delete_api_key(&account);
        let got = get_api_key(&account).unwrap();
        assert!(got.is_none());
    }

    #[test]
    #[ignore = "hits OS Keychain, run explicitly with --ignored"]
    fn delete_missing_entry_is_noop() {
        let account = format!("__test_delete_noop_{}", std::process::id());
        delete_api_key(&account).unwrap();
    }

    #[test]
    fn service_name_matches_bundle_id() {
        assert_eq!(SERVICE_NAME, "net.alicelaw.text-to-print");
    }
}
