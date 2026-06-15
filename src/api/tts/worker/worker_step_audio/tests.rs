use super::synthesize_step_audio;
use crate::api::tts::manager::TtsManager;
use crate::api::tts::worker::audio_utils::read_wav_i16;
use crate::api::tts::types::{QueuedRequest, TtsRequest, TtsRequestProfile};
use crate::config::{StepAudioSettings, TtsMethod};

#[test]
fn read_wav_i16_accepts_step_audio_smoke_output() {
    let path = std::env::temp_dir().join("step-audio-smoke.wav");
    if !path.is_file() {
        eprintln!(
            "skipping Step Audio smoke WAV test; '{}' is missing",
            path.display()
        );
        return;
    }

    let (samples, sample_rate) = read_wav_i16(&path, "Step Audio", false).expect("read smoke wav");
    assert_eq!(sample_rate, 24_000);
    assert!(
        samples.len() >= (sample_rate as usize / 2),
        "smoke WAV is too short: {} samples at {sample_rate} Hz",
        samples.len()
    );
}

#[test]
fn synthesize_step_audio_e2e_when_enabled() {
    if std::env::var("SGT_STEP_AUDIO_E2E").as_deref() != Ok("1") {
        eprintln!("skipping Step Audio e2e test; set SGT_STEP_AUDIO_E2E=1 to run it");
        return;
    }

    let request = QueuedRequest {
        req: TtsRequest {
            _id: 9_001,
            text: "Step Audio worker end to end test.".to_string(),
            hwnd: 0,
            is_realtime: false,
            profile: Some(TtsRequestProfile {
                method: TtsMethod::StepAudioEditX,
                gemini_model: String::new(),
                gemini_voice: String::new(),
                gemini_speed: String::new(),
                gemini_instruction: String::new(),
                gemini_language_conditions: Vec::new(),
                gemini_parallel_requests: 2,
                google_speed: String::new(),
                edge_voice: String::new(),
                edge_settings: Default::default(),
                step_audio_settings: StepAudioSettings::default(),
                magpie_settings: Default::default(),
                kokoro_settings: Default::default(),
                supertonic_settings: Default::default(),
                vieneu_settings: Default::default(),
                language_code_override: Some("eng".to_string()),
            }),
        },
        generation: 0,
    };

    let manager = std::sync::Arc::new(TtsManager::new());
    let (samples, sample_rate) =
        synthesize_step_audio(manager, &request).expect("synthesize step audio");
    assert_eq!(sample_rate, 24_000);
    assert!(
        samples.len() >= (sample_rate as usize / 2),
        "Step Audio e2e output is too short: {} samples at {sample_rate} Hz",
        samples.len()
    );
}
