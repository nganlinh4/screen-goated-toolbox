use anyhow::Result;

const PUBLIC_KEY_HEX: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/component-delivery/update-catalog-p256-public-key.hex"
));

const LABEL: &str = "component catalog";

pub(super) fn verify(payload: &[u8], signature: &[u8]) -> Result<()> {
    let public_key = crate::crypto::decode_hex(PUBLIC_KEY_HEX.trim(), LABEL)?;
    crate::crypto::verify_p256_sha256(&public_key, payload, signature, LABEL)
}

#[cfg(test)]
pub(super) fn verify_test_key(public_key: &[u8], payload: &[u8], signature: &[u8]) -> Result<()> {
    crate::crypto::verify_p256_sha256(public_key, payload, signature, LABEL)
}
