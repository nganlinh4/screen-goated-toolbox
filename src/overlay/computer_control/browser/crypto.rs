//! Browser pairing uses shared message authentication and fresh random nonces.

pub(super) use crate::crypto::{constant_time_eq as ct_eq, hmac_sha256_hex};

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// A random `n_bytes`-byte value as lowercase hex (for the shared secret / nonces).
pub(super) fn random_hex(n_bytes: usize) -> anyhow::Result<String> {
    let mut buf = vec![0u8; n_bytes];
    getrandom::fill(&mut buf).map_err(|e| anyhow::anyhow!("secure randomness unavailable: {e}"))?;
    Ok(hex(&buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_matches_rfc4231_vector_2() {
        // RFC 4231 test case 2 - the wire contract the extension's WebCrypto
        // HMAC-SHA-256 must also satisfy, or pairing silently never authenticates.
        assert_eq!(
            hmac_sha256_hex(b"Jefe", b"what do ya want for nothing?"),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn hmac_keys_longer_than_the_block_are_hashed() {
        // Key > 64 bytes takes the SHA-256(key) path; just assert it's stable and
        // distinct from a short key, so a regression there can't pass silently.
        let long = vec![0xaau8; 80];
        let a = hmac_sha256_hex(&long, b"nonce");
        assert_eq!(a.len(), 64);
        assert_ne!(a, hmac_sha256_hex(b"\xaa", b"nonce"));
    }

    #[test]
    fn ct_eq_distinguishes_and_handles_length() {
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"abcd"));
    }
}
