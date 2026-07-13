//! Canonical Gemini Live client-message construction.
//!
//! Transport ownership and feature policy stay elsewhere; these builders keep
//! raw and setup-gated callers byte-for-byte aligned while migration proceeds.

use base64::{Engine as _, engine::general_purpose};
use serde_json::{Value, json};

pub fn realtime_audio_pcm(samples: &[i16], sample_rate: u32) -> Value {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }

    realtime_audio_bytes(&bytes, sample_rate)
}

pub fn realtime_audio_bytes(bytes: &[u8], sample_rate: u32) -> Value {
    json!({
        "realtimeInput": {
            "audio": {
                "data": general_purpose::STANDARD.encode(bytes),
                "mimeType": format!("audio/pcm;rate={sample_rate}")
            }
        }
    })
}

pub fn audio_stream_end() -> Value {
    json!({
        "realtimeInput": {
            "audioStreamEnd": true
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_payload_is_little_endian_with_explicit_rate() {
        let payload = realtime_audio_pcm(&[0x0102, -2], 16_000);
        assert_eq!(payload["realtimeInput"]["audio"]["data"], "AgH+/w==");
        assert_eq!(
            payload["realtimeInput"]["audio"]["mimeType"],
            "audio/pcm;rate=16000"
        );
    }

    #[test]
    fn byte_payload_is_not_reencoded_as_samples() {
        let payload = realtime_audio_bytes(&[1, 2, 3], 24_000);
        assert_eq!(payload["realtimeInput"]["audio"]["data"], "AQID");
        assert_eq!(
            payload["realtimeInput"]["audio"]["mimeType"],
            "audio/pcm;rate=24000"
        );
    }

    #[test]
    fn stream_end_is_a_structural_realtime_input() {
        assert_eq!(
            audio_stream_end(),
            json!({"realtimeInput": {"audioStreamEnd": true}})
        );
    }
}
