use sha2::{Digest as _, Sha256};

/// HMAC-SHA256 encoded as lowercase hexadecimal.
pub(crate) fn hmac_sha256_hex(key: &[u8], message: &[u8]) -> String {
    let mut block = [0_u8; 64];
    if key.len() > block.len() {
        let digest = Sha256::digest(key);
        block[..digest.len()].copy_from_slice(&digest);
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner_pad = [0x36; 64];
    let mut outer_pad = [0x5c; 64];
    for index in 0..64 {
        inner_pad[index] ^= block[index];
        outer_pad[index] ^= block[index];
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(message);
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner.finalize());
    outer
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(crate) fn constant_time_eq(actual: &[u8], expected: &[u8]) -> bool {
    actual.len() == expected.len()
        && actual
            .iter()
            .zip(expected)
            .fold(0_u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_matches_rfc4231_short_and_long_keys() {
        assert_eq!(
            hmac_sha256_hex(b"Jefe", b"what do ya want for nothing?"),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        assert_eq!(
            hmac_sha256_hex(
                &[0xaa; 131],
                b"Test Using Larger Than Block-Size Key - Hash Key First"
            ),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
        assert!(constant_time_eq(b"same", b"same"));
        assert!(!constant_time_eq(b"same", b"sand"));
        assert!(!constant_time_eq(b"same", b"same-more"));
    }
}
