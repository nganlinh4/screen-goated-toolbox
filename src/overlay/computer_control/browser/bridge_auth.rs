//! Authenticated browser-bridge sessions and replay-resistant message frames.

use anyhow::{Result, anyhow, ensure};
use serde_json::{Value, json};

use super::crypto;

pub(super) const PROTOCOL: u64 = 8;
const DOMAIN: &str = "sgt-browser-v8";

pub(super) fn durable_key(bootstrap: &str) -> String {
    crypto::hmac_sha256_hex(bootstrap.as_bytes(), b"sgt-browser-v8|durable")
}

pub(super) fn bootstrap_key(bootstrap: &str) -> String {
    crypto::hmac_sha256_hex(bootstrap.as_bytes(), b"sgt-browser-v8|bootstrap-handshake")
}

pub(super) fn valid_nonce(nonce: &str) -> bool {
    nonce.len() == 64
        && nonce
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(super) fn proof(key: &str, side: &str, key_kind: &str, client: &str, server: &str) -> String {
    crypto::hmac_sha256_hex(
        key.as_bytes(),
        format!("{DOMAIN}|{side}|{key_kind}|{client}|{server}").as_bytes(),
    )
}

pub(super) struct Session {
    key: Vec<u8>,
    client: String,
    server: String,
    sent: u64,
    received: u64,
}

impl Session {
    pub(super) fn new(key: &str, client: &str, server: &str) -> Self {
        let key = crypto::hmac_sha256_hex(
            key.as_bytes(),
            format!("{DOMAIN}|aead|{client}|{server}").as_bytes(),
        );
        Self {
            key: crate::crypto::decode_hex(&key, "browser session key")
                .expect("HMAC always encodes valid hex"),
            client: client.into(),
            server: server.into(),
            sent: 0,
            received: 0,
        }
    }

    fn crypt(&self, direction: &str, seq: u64, input: &[u8], encrypt: bool) -> Result<Vec<u8>> {
        let mut nonce = [0u8; 12];
        nonce[..4].copy_from_slice(&(if direction == "s2c" { 0u32 } else { 1u32 }).to_be_bytes());
        nonce[4..].copy_from_slice(&seq.to_be_bytes());
        let aad = format!(
            "{DOMAIN}|frame|{direction}|{}|{}|{seq}",
            self.client, self.server
        );
        super::bridge_cipher::crypt(&self.key, &nonce, aad.as_bytes(), input, encrypt)
    }

    pub(super) fn seal(&mut self, body: &Value) -> Result<Value> {
        let seq = self
            .sent
            .checked_add(1)
            .ok_or_else(|| anyhow!("browser frame sequence exhausted"))?;
        let body = serde_json::to_vec(body)?;
        let ciphertext = self.crypt("s2c", seq, &body, true)?;
        self.sent = seq;
        Ok(json!({"type":"secure", "seq":seq, "ciphertext":hex(&ciphertext)}))
    }

    pub(super) fn open(&mut self, frame: &Value) -> Result<Value> {
        ensure!(
            frame.get("type").and_then(Value::as_str) == Some("secure"),
            "unauthenticated browser frame"
        );
        let seq = frame
            .get("seq")
            .and_then(Value::as_u64)
            .ok_or_else(|| anyhow!("invalid browser frame sequence"))?;
        ensure!(
            self.received.checked_add(1) == Some(seq),
            "browser frame sequence mismatch"
        );
        let encoded = frame
            .get("ciphertext")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("invalid browser frame ciphertext"))?;
        ensure!(
            encoded.len() <= 32 * 1024 * 1024,
            "browser frame exceeds byte limit"
        );
        let ciphertext = crate::crypto::decode_hex(encoded, "browser frame")?;
        let body = self.crypt("c2s", seq, &ciphertext, false)?;
        let decoded = serde_json::from_slice(&body)?;
        self.received = seq;
        Ok(decoded)
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proofs_bind_direction_key_and_fresh_nonces() {
        let server = proof("key", "server", "secret", "client", "server");
        assert_ne!(server, proof("key", "client", "secret", "client", "server"));
        assert_ne!(
            server,
            proof("key", "server", "bootstrap", "client", "server")
        );
        assert_ne!(server, proof("key", "server", "secret", "other", "server"));
        assert!(valid_nonce(&"a".repeat(64)));
        assert!(!valid_nonce(&"z".repeat(64)));
    }

    #[test]
    fn frames_reject_replay_tampering_and_wrong_session() {
        let mut session = Session::new("key", "client", "server");
        let body = br#"{"type":"event","value":1}"#;
        let frame = json!({"type":"secure","seq":1,"ciphertext":hex(&session.crypt("c2s",1,body,true).unwrap())});
        let mut altered = frame.clone();
        altered["ciphertext"] = json!("00".repeat(body.len() + 16));
        assert!(session.open(&altered).is_err());
        let mut other = Session::new("key", "other", "server");
        assert!(other.open(&frame).is_err());
        assert_eq!(session.open(&frame).unwrap()["type"], "event");
        assert!(session.open(&frame).is_err());
        assert!(session.open(&json!({"type":"event"})).is_err());
    }

    #[test]
    fn aes_frame_matches_the_browser_wire_vector() {
        let mut session = Session::new(
            &bootstrap_key("bootstrap-key"),
            &"11".repeat(32),
            &"22".repeat(32),
        );
        let frame = session.seal(&json!({"type":"ready"})).unwrap();
        assert_eq!(
            frame["ciphertext"],
            "70832ac63b7b897bf233303c0438da97d82c1df30131e43e93c0bff04ecfc4e7"
        );
    }
}
