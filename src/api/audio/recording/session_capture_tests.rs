use super::*;
use crate::overlay::result::ChainCancelToken;

#[test]
fn recorded_wav_is_compact_speech_pcm_independent_of_device_format() {
    let spec = transcription_wav_spec();
    assert_eq!((spec.channels, spec.sample_rate), (1, 16_000));
    let minute = 16_000 * 60;
    let budget = crate::api::audio::retention::CaptureBudget::new(spec.sample_rate, spec.channels);
    assert!(budget.claim(minute));
    let mut wav = Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(&mut wav, spec).unwrap();
    for _ in 0..minute {
        writer.write_sample(0i16).unwrap();
    }
    writer.finalize().unwrap();
    // One minute of 48 kHz stereo device audio was 11.5 MB; this is 1.9 MB.
    assert!(wav.into_inner().len() < 2_000_000);
}

#[test]
fn restarting_does_not_revive_cancelled_capture() {
    let stop = AtomicBool::new(false);
    let old = ChainCancelToken::new();
    assert!(should_capture(&stop, &old));
    old.cancel();
    stop.store(true, Ordering::SeqCst);
    stop.store(false, Ordering::SeqCst);
    let next = ChainCancelToken::new();
    assert!(!should_capture(&stop, &old));
    assert!(should_capture(&stop, &next));
}
