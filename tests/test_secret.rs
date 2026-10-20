//! OS keyring round-trip for the LLM API key. Opt-in: requires a working
//! keyring daemon (Windows Credential Manager, macOS Keychain, or a Linux
//! Secret Service). Run with `cargo test --test test_secret -- --ignored`.

use logscope::core::secret;

#[test]
#[ignore = "requires a working OS keyring"]
fn api_key_round_trip() {
    assert!(secret::keyring_available(), "OS keyring not reachable");

    secret::store_api_key("sk-test-1234").expect("store should succeed");
    assert_eq!(secret::load_api_key().as_deref(), Some("sk-test-1234"));

    // Empty key deletes the entry.
    secret::store_api_key("").expect("delete should succeed");
    assert_eq!(secret::load_api_key(), None);
}
