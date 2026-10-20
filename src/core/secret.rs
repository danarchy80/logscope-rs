//! OS keyring storage for sensitive values (the LLM API key).
//!
//! Backed by the platform credential store: Windows Credential Manager,
//! macOS Keychain, or the Linux Secret Service. The key is never written to
//! the app's plaintext settings file. When no keyring is reachable the key
//! simply stays in memory for the session and is not persisted.

use keyring::Entry;

/// Service/namespace used for all LogScope keyring entries.
pub const SERVICE: &str = "logscope";
/// Account name for the LLM API key entry.
const API_KEY_ACCOUNT: &str = "llm_api_key";

fn api_key_entry() -> Result<Entry, String> {
    Entry::new(SERVICE, API_KEY_ACCOUNT).map_err(|e| format!("keyring entry: {e}"))
}

/// True when the OS keyring actually persists writes: performs a real
/// set/get/delete round-trip on a throwaway probe entry (some backends
/// accept writes but never persist them), so a stored key is never touched.
pub fn keyring_available() -> bool {
    let Ok(entry) = Entry::new(SERVICE, "availability_probe") else {
        return false;
    };
    const PROBE: &str = "logscope-probe";
    if entry.set_password(PROBE).is_err() {
        return false;
    }
    let ok = entry.get_password().map(|v| v == PROBE).unwrap_or(false);
    let _ = entry.delete_credential();
    ok
}

/// Store the API key, or delete the stored entry when `key` is empty.
/// Returns `Err` only when the keyring is present but the write failed.
pub fn store_api_key(key: &str) -> Result<(), String> {
    let entry = api_key_entry()?;
    if key.is_empty() {
        // Deleting a non-existent entry is fine.
        let _ = entry.delete_credential();
        return Ok(());
    }
    entry
        .set_password(key)
        .map_err(|e| format!("keyring store: {e}"))
}

/// Load the stored API key, or `None` when absent / keyring unavailable.
pub fn load_api_key() -> Option<String> {
    let entry = api_key_entry().ok()?;
    entry.get_password().ok().filter(|s| !s.is_empty())
}
