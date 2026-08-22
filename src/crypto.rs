//! Shared P-256 signature verification.
//!
//! Delivery manifests (the component catalog and host app update feed) use the
//! update-delivery key. The provider availability feed only influences model
//! routing and uses a separate, lower-trust key. All share this verification
//! implementation because it is unsafe Windows crypto and a second
//! transcription would be a second place to get it wrong.
//!
//! The two tracked key encodings differ only in representation. The update key
//! is uncompressed SEC1, 65 bytes beginning `0x04`; the availability key is the
//! raw 64-byte coordinate pair. Both reduce to the X‖Y that Windows wants, so
//! both are accepted here and normalised.

use anyhow::{Result, bail};
use sha2::{Digest, Sha256};
use windows::Win32::Security::Cryptography::{
    BCRYPT_ALG_HANDLE, BCRYPT_ECCPUBLIC_BLOB, BCRYPT_ECDSA_P256_ALGORITHM,
    BCRYPT_ECDSA_PUBLIC_P256_MAGIC, BCRYPT_FLAGS, BCRYPT_KEY_HANDLE,
    BCRYPT_OPEN_ALGORITHM_PROVIDER_FLAGS, BCryptCloseAlgorithmProvider, BCryptDestroyKey,
    BCryptImportKeyPair, BCryptOpenAlgorithmProvider, BCryptVerifySignature,
};

const COORDINATE_PAIR_LEN: usize = 64;
const UNCOMPRESSED_LEN: usize = 65;
const SIGNATURE_LEN: usize = 64;

struct Algorithm(BCRYPT_ALG_HANDLE);
struct Key(BCRYPT_KEY_HANDLE);

impl Drop for Algorithm {
    fn drop(&mut self) {
        unsafe {
            let _ = BCryptCloseAlgorithmProvider(self.0, 0);
        }
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            let _ = BCryptDestroyKey(self.0);
        }
    }
}

/// Verifies a raw P-256 signature over the SHA-256 digest of `payload`.
///
/// `public_key` may be the 64-byte coordinate pair or the 65-byte uncompressed
/// form. `label` names the feed in errors, so a failure says which one failed.
pub(crate) fn verify_p256_sha256(
    public_key: &[u8],
    payload: &[u8],
    signature: &[u8],
    label: &str,
) -> Result<()> {
    let coordinates = match public_key.len() {
        COORDINATE_PAIR_LEN => public_key,
        UNCOMPRESSED_LEN if public_key[0] == 4 => &public_key[1..],
        _ => bail!("{label} public key shape is invalid"),
    };
    if signature.len() != SIGNATURE_LEN {
        bail!("{label} signature shape is invalid");
    }

    let mut blob = Vec::with_capacity(8 + COORDINATE_PAIR_LEN);
    blob.extend_from_slice(&BCRYPT_ECDSA_PUBLIC_P256_MAGIC.to_le_bytes());
    blob.extend_from_slice(&32u32.to_le_bytes());
    blob.extend_from_slice(coordinates);

    let mut algorithm = BCRYPT_ALG_HANDLE::default();
    let opened = unsafe {
        BCryptOpenAlgorithmProvider(
            &mut algorithm,
            BCRYPT_ECDSA_P256_ALGORITHM,
            None,
            BCRYPT_OPEN_ALGORITHM_PROVIDER_FLAGS(0),
        )
    };
    if opened.is_err() {
        bail!("Windows could not open the {label} signature provider");
    }
    let algorithm = Algorithm(algorithm);

    let mut key = BCRYPT_KEY_HANDLE::default();
    let imported = unsafe {
        BCryptImportKeyPair(algorithm.0, None, BCRYPT_ECCPUBLIC_BLOB, &mut key, &blob, 0)
    };
    if imported.is_err() {
        bail!("{label} public key could not be imported");
    }
    let key = Key(key);

    let digest = Sha256::digest(payload);
    let verified =
        unsafe { BCryptVerifySignature(key.0, None, &digest, signature, BCRYPT_FLAGS(0)) };
    if verified.is_err() {
        bail!("{label} signature is invalid");
    }
    Ok(())
}

/// Decodes a hex-encoded public key.
pub(crate) fn decode_hex(value: &str, label: &str) -> Result<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        bail!("{label} public key encoding is invalid");
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair)?;
            Ok(u8::from_str_radix(text, 16)?)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // A key and signature produced by the signing scripts, so the two encodings
    // are exercised against real output rather than a hand-built blob.
    const KEY_XY: &str = "3f6887d6aaf08030e8e98b5108e033cb93de843a7856c6d21a987bb74f6e2048dbb76fac3714c2a27c75f887f42aca94ad0b30f255540b3fc0ba7c7dea0fcd3c";

    #[test]
    fn both_public_key_encodings_are_accepted() {
        let raw = decode_hex(KEY_XY, "test").expect("hex decodes");
        assert_eq!(raw.len(), COORDINATE_PAIR_LEN);
        let mut uncompressed = vec![4u8];
        uncompressed.extend_from_slice(&raw);
        // Neither call can succeed without a matching signature, but a shape
        // rejection is a different error than a verification failure.
        for key in [raw.as_slice(), uncompressed.as_slice()] {
            let error = verify_p256_sha256(key, b"payload", &[0u8; SIGNATURE_LEN], "test")
                .expect_err("an all-zero signature cannot verify");
            assert!(
                !error.to_string().contains("shape is invalid"),
                "shape was rejected for a valid encoding: {error}"
            );
        }
    }

    #[test]
    fn malformed_shapes_are_rejected_before_any_crypto() {
        let raw = decode_hex(KEY_XY, "test").expect("hex decodes");
        for (key, signature) in [
            (vec![0u8; 10], vec![0u8; SIGNATURE_LEN]),
            (raw.clone(), vec![0u8; 10]),
            // 65 bytes without the uncompressed marker is not a key we accept.
            ([vec![9u8], raw.clone()].concat(), vec![0u8; SIGNATURE_LEN]),
        ] {
            let error = verify_p256_sha256(&key, b"payload", &signature, "test")
                .expect_err("malformed input must fail");
            assert!(error.to_string().contains("shape is invalid"), "{error}");
        }
    }

    #[test]
    fn odd_length_hex_is_rejected() {
        assert!(decode_hex("abc", "test").is_err());
    }
}
