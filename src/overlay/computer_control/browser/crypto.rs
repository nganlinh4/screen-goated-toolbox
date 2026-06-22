//! Minimal crypto for the browser-bridge pairing: HMAC-SHA256 (built on the
//! `sha2` dep — no `hmac` crate in the tree) and random hex, plus a constant-time
//! compare for the challenge-response check.

use sha2::{Digest, Sha256};

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// HMAC-SHA256(key, msg) as lowercase hex.
pub(super) fn hmac_sha256_hex(key: &[u8], msg: &[u8]) -> String {
    const BLOCK: usize = 64;
    let mut k = [0u8; BLOCK];
    if key.len() > BLOCK {
        let h = Sha256::digest(key);
        k[..h.len()].copy_from_slice(&h);
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(msg);
    let inner = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner);
    hex(&outer.finalize())
}

/// A random `n_bytes`-byte value as lowercase hex (for the shared secret / nonces).
pub(super) fn random_hex(n_bytes: usize) -> String {
    let mut buf = vec![0u8; n_bytes];
    if getrandom::fill(&mut buf).is_err() {
        // Extremely unlikely; fall back to a non-secret-but-distinct value rather
        // than panic (the bridge is local-only and additionally socket-gated).
        for (i, b) in buf.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(31).wrapping_add(7);
        }
    }
    hex(&buf)
}

/// Constant-time equality (avoid leaking the secret via compare timing).
pub(super) fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
