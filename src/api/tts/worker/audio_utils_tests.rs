use super::*;
use base64::Engine;

// A synthetic 50 ms silent mono MP3, with no user media.
const SILENT_MP3: &str = concat!(
    "SUQzBAAAAAAAI1RTU0UAAAAPAAADTGF2ZjYyLjEyLjEwMgAAAAAAAAAAAAAA//NkxAAAAANIAAAAAExBTUVVVVVMQU1FMy4xMDBV",
    "VVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVV",
    "VVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVV//NkxHwAAANIAAAAAFVVVVVVVVVMQU1FMy4xMDBVVVVVVVVV",
    "VVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVV",
    "VVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVV//NkxHwAAANIAAAAAFVVVVVVVVVMQU1FMy4xMDBVVVVVVVVVVVVVVVVV",
    "VVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVV",
    "VVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVV//NkxHwAAANIAAAAAFVVVVVVVVVMQU1FMy4xMDBVVVVVVVVVVVVVVVVVVVVVVVVV",
    "VVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVV",
    "VVVVVVVVVVVVVVVVVVVVVVVVVVVV//NkxHwAAANIAAAAAFVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVV",
    "VVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVV",
    "VVVVVVVVVVVVVVVVVVVV",
);

#[test]
fn edge_decoder_accepts_complete_synthetic_audio() {
    let mp3 = base64::engine::general_purpose::STANDARD
        .decode(SILENT_MP3)
        .unwrap();
    let (samples, sample_rate) = decode_edge_mp3_to_pcm(mp3, || false).unwrap().unwrap();
    assert!(!samples.is_empty());
    assert_eq!(sample_rate, 24_000);
}

#[test]
fn edge_decoder_rejects_empty_invalid_and_incomplete_first_frame() {
    for bytes in [vec![], b"not an mp3".to_vec(), vec![0xff, 0xf3, 0x64, 0xc4]] {
        assert!(decode_edge_mp3_to_pcm(bytes, || false).is_err());
    }
}

#[test]
fn edge_decoder_cancellation_returns_no_samples() {
    assert!(decode_edge_mp3_to_pcm(vec![0], || true).unwrap().is_none());
}

#[test]
fn legacy_decoder_keeps_empty_invalid_input_behavior() {
    let mut samples = Vec::new();
    let mut rate = 24_000;
    assert!(decode_mp3_to_pcm(vec![0], &mut samples, &mut rate, || {
        false
    }));
    assert!(samples.is_empty());
}
