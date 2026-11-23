//! At-rest protection for the provider API keys in the config file.
//!
//! Keys are wrapped with Windows DPAPI (current-user scope) when the config is
//! written and unwrapped when it is read, so the in-memory `Config` always holds
//! plaintext and no other serializer of `Config` changes. A sealed value is
//! stored as `dpapi:v1:<base64>`; an empty key stays empty. Plaintext values
//! written by older builds still load, and the loader re-saves them sealed.
//! A value that cannot be unsealed (for example a profile copied to another
//! machine or user) loads as an empty key with a logged warning; every other
//! setting is unaffected, and saves keep writing the stored value back until
//! the user enters a replacement, so a failed unseal never erases the key.

use crate::config::Config;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptUnprotectData,
};

const SEALED_PREFIX: &str = "dpapi:v1:";

/// Binds sealed values to this purpose, so a DPAPI blob made by another
/// program for the same user does not unseal as an API key.
const ENTROPY: &[u8] = b"screen-goated-toolbox/config/api-key/v1";

/// The config fields that hold provider credentials, by JSON field name.
fn secret_fields(config: &mut Config) -> [(&'static str, &mut String); 4] {
    [
        ("api_key", &mut config.api_key),
        ("gemini_api_key", &mut config.gemini_api_key),
        ("openrouter_api_key", &mut config.openrouter_api_key),
        ("nvidia_api_key", &mut config.nvidia_api_key),
    ]
}

/// Stored values that could not be unsealed when the config was read, by
/// position in `secret_fields`.
#[derive(Debug, Default)]
#[cfg_attr(feature = "recorder-worker", allow(dead_code))]
pub(super) struct UnreadableKeys([Option<String>; 4]);

#[cfg(not(feature = "recorder-worker"))]
impl UnreadableKeys {
    const NONE: Self = Self([None, None, None, None]);
}

pub(super) struct Unsealed {
    /// A key was stored as legacy plaintext, so the caller should re-save
    /// the file to hold it sealed.
    pub(super) legacy_plaintext: bool,
    pub(super) unreadable: UnreadableKeys,
}

/// Replace every sealed key in a config just read from disk with its
/// plaintext, setting aside any stored value that cannot be unsealed.
pub(super) fn unseal_api_keys(config: &mut Config) -> Unsealed {
    let mut unsealed = Unsealed {
        legacy_plaintext: false,
        unreadable: UnreadableKeys::default(),
    };
    for (index, (field, value)) in secret_fields(config).into_iter().enumerate() {
        let Some(encoded) = value.strip_prefix(SEALED_PREFIX) else {
            unsealed.legacy_plaintext |= !value.is_empty();
            continue;
        };
        match unseal(encoded) {
            Ok(plaintext) => *value = plaintext,
            Err(reason) => {
                crate::log_info!(
                    "[config] WARNING: stored {field} could not be decrypted for this                      Windows user ({reason}); it must be entered again here, and the                      stored value is kept until it is"
                );
                unsealed.unreadable.0[index] = Some(std::mem::take(value));
            }
        }
    }
    unsealed
}

/// What the config read at startup could not unseal; every save consults it.
#[cfg(not(feature = "recorder-worker"))]
static UNREADABLE_AT_LOAD: std::sync::Mutex<UnreadableKeys> =
    std::sync::Mutex::new(UnreadableKeys::NONE);

#[cfg(not(feature = "recorder-worker"))]
pub(super) fn keep_unreadable_for_saves(keys: UnreadableKeys) {
    *UNREADABLE_AT_LOAD
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = keys;
}

/// A copy of `config` sealed for the config file, keeping stored values that
/// could not be unsealed at startup.
#[cfg(not(feature = "recorder-worker"))]
pub(super) fn sealed_for_save(config: &Config) -> std::io::Result<Config> {
    let mut unreadable = UNREADABLE_AT_LOAD
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    sealed_for_disk(config, &mut unreadable)
}

/// A copy of `config` whose keys are sealed for writing to disk. An empty key
/// whose stored value could not be unsealed writes that value back unchanged;
/// a key the user entered replaces it for good.
#[cfg(not(feature = "recorder-worker"))]
pub(super) fn sealed_for_disk(
    config: &Config,
    unreadable: &mut UnreadableKeys,
) -> std::io::Result<Config> {
    let mut sealed = config.clone();
    for (index, (field, value)) in secret_fields(&mut sealed).into_iter().enumerate() {
        if value.is_empty() {
            if let Some(stored) = &unreadable.0[index] {
                value.clone_from(stored);
            }
            continue;
        }
        let encoded = seal(value.as_bytes())
            .map_err(|e| std::io::Error::other(format!("could not encrypt {field}: {e}")))?;
        *value = format!("{SEALED_PREFIX}{encoded}");
        unreadable.0[index] = None;
    }
    Ok(sealed)
}

fn blob(bytes: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr().cast_mut(),
    }
}

/// Copy a DPAPI output blob and release the buffer the system allocated.
fn take_output(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
    if out.pbData.is_null() {
        return Vec::new();
    }
    // SAFETY: on success DPAPI returns `cbData` readable bytes at `pbData`,
    // allocated with LocalAlloc; they are copied before being freed once.
    unsafe {
        let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(out.pbData.cast())));
        bytes
    }
}

#[cfg(not(feature = "recorder-worker"))]
fn seal(plaintext: &[u8]) -> windows::core::Result<String> {
    use windows::Win32::Security::Cryptography::CryptProtectData;
    let input = blob(plaintext);
    let entropy = blob(ENTROPY);
    let mut out = CRYPT_INTEGER_BLOB::default();
    // SAFETY: the input and entropy blobs borrow slices that outlive the call,
    // and `out` is only read after DPAPI reports success.
    unsafe {
        CryptProtectData(
            &input,
            windows::core::PCWSTR::null(),
            Some(&entropy),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )?;
    }
    Ok(BASE64.encode(take_output(out)))
}

fn unseal(encoded: &str) -> Result<String, String> {
    let sealed = BASE64
        .decode(encoded.trim())
        .map_err(|e| format!("invalid encoding: {e}"))?;
    let input = blob(&sealed);
    let entropy = blob(ENTROPY);
    let mut out = CRYPT_INTEGER_BLOB::default();
    // SAFETY: as in `seal`; `sealed` and `ENTROPY` outlive the call.
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            Some(&entropy),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
        .map_err(|e| e.message())?;
    }
    String::from_utf8(take_output(out)).map_err(|_| "decrypted value is not text".to_string())
}

#[cfg(all(test, not(feature = "recorder-worker")))]
#[path = "io_secrets_tests.rs"]
mod tests;
