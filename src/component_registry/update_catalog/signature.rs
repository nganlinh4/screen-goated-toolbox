use anyhow::{Result, bail};
use sha2::{Digest, Sha256};
use windows::Win32::Security::Cryptography::{
    BCRYPT_ALG_HANDLE, BCRYPT_ECCPUBLIC_BLOB, BCRYPT_ECDSA_P256_ALGORITHM,
    BCRYPT_ECDSA_PUBLIC_P256_MAGIC, BCRYPT_FLAGS, BCRYPT_KEY_HANDLE,
    BCRYPT_OPEN_ALGORITHM_PROVIDER_FLAGS, BCryptCloseAlgorithmProvider, BCryptDestroyKey,
    BCryptImportKeyPair, BCryptOpenAlgorithmProvider, BCryptVerifySignature,
};

const PUBLIC_KEY_HEX: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/component-delivery/update-catalog-p256-public-key.hex"
));

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

pub(super) fn verify(payload: &[u8], signature: &[u8]) -> Result<()> {
    let public_key = decode_hex(PUBLIC_KEY_HEX.trim())?;
    verify_with_key(&public_key, payload, signature)
}

fn verify_with_key(public_key: &[u8], payload: &[u8], signature: &[u8]) -> Result<()> {
    if public_key.len() != 65 || public_key[0] != 4 || signature.len() != 64 {
        bail!("component catalog signature shape is invalid");
    }
    let mut blob = Vec::with_capacity(72);
    blob.extend_from_slice(&BCRYPT_ECDSA_PUBLIC_P256_MAGIC.to_le_bytes());
    blob.extend_from_slice(&32u32.to_le_bytes());
    blob.extend_from_slice(&public_key[1..]);

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
        bail!("Windows could not open the component catalog signature provider");
    }
    let algorithm = Algorithm(algorithm);
    let mut key = BCRYPT_KEY_HANDLE::default();
    let imported = unsafe {
        BCryptImportKeyPair(algorithm.0, None, BCRYPT_ECCPUBLIC_BLOB, &mut key, &blob, 0)
    };
    if imported.is_err() {
        bail!("component catalog public key could not be imported");
    }
    let key = Key(key);
    let digest = Sha256::digest(payload);
    let verified =
        unsafe { BCryptVerifySignature(key.0, None, &digest, signature, BCRYPT_FLAGS(0)) };
    if verified.is_err() {
        bail!("component catalog signature is invalid");
    }
    Ok(())
}

fn decode_hex(value: &str) -> Result<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        bail!("component catalog public key encoding is invalid");
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
pub(super) fn verify_test_key(public_key: &[u8], payload: &[u8], signature: &[u8]) -> Result<()> {
    verify_with_key(public_key, payload, signature)
}
