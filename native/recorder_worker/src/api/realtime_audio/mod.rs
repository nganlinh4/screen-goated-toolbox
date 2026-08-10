#[path = "../../../../../src/api/realtime_audio/kokoro_assets.rs"]
pub(crate) mod kokoro_assets;
#[path = "../../../../../src/api/realtime_audio/local_asr_worker.rs"]
pub(crate) mod local_asr_worker;
#[path = "../../../../../src/api/realtime_audio/magpie_assets.rs"]
pub(crate) mod magpie_assets;
#[path = "../../../../../src/api/realtime_audio/magpie_runtime.rs"]
pub(crate) mod magpie_runtime;
#[path = "../../../../../src/api/realtime_audio/model_component_assets.rs"]
mod model_component_assets;
#[path = "../../../../../src/api/realtime_audio/model_loader.rs"]
pub(crate) mod model_loader;
#[path = "../../../../../src/api/realtime_audio/parakeet_tdt_assets.rs"]
pub(crate) mod parakeet_tdt_assets;
pub(crate) mod qwen3;
#[path = "../../../../../src/api/realtime_audio/s2s.rs"]
pub(crate) mod s2s;
pub(crate) mod sherpa_onnx;
pub(crate) mod state {
    #[derive(Default)]
    pub(crate) struct RealtimeState {
        pub(crate) is_downloading: bool,
        pub(crate) download_title: String,
        pub(crate) download_message: String,
        pub(crate) download_progress: f32,
    }
}
#[path = "../../../../../src/api/realtime_audio/step_audio_assets.rs"]
pub(crate) mod step_audio_assets;
#[path = "../../../../../src/api/realtime_audio/step_audio_runtime.rs"]
pub(crate) mod step_audio_runtime;
#[path = "../../../../../src/api/realtime_audio/supertonic_assets.rs"]
pub(crate) mod supertonic_assets;
#[path = "../../../../../src/api/realtime_audio/transcript_state.rs"]
pub(crate) mod transcript_state;
#[path = "../../../../../src/api/realtime_audio/vieneu_assets.rs"]
pub(crate) mod vieneu_assets;
#[path = "../../../../../src/api/realtime_audio/vieneu_runtime.rs"]
pub(crate) mod vieneu_runtime;
#[path = "../../../../../src/api/realtime_audio/websocket.rs"]
pub(crate) mod websocket;
use windows::Win32::UI::WindowsAndMessaging::WM_APP;

pub(crate) use state::RealtimeState;

pub(crate) const WM_DOWNLOAD_PROGRESS: u32 = WM_APP + 204;

pub(crate) fn translate_with_google_gtx(text: &str, target_lang: &str) -> Option<String> {
    let target = target_lang.trim();
    let target_code = if (2..=3).contains(&target.len())
        && target
            .chars()
            .all(|character| character.is_ascii_alphabetic())
    {
        target.to_ascii_lowercase()
    } else {
        isolang::Language::from_name(target)
            .and_then(|language| language.to_639_1())
            .map(str::to_owned)
            .unwrap_or_else(|| "en".to_owned())
    };
    let url = format!(
        "https://translate.googleapis.com/translate_a/single?client=gtx&sl=auto&tl={target_code}&dt=t&q={}",
        urlencoding::encode(text)
    );
    let response = client::UREQ_AGENT
        .get(&url)
        .header("User-Agent", "Mozilla/5.0")
        .call()
        .ok()?;
    let value = response.into_body().read_json::<serde_json::Value>().ok()?;
    let translated = value
        .get(0)?
        .as_array()?
        .iter()
        .filter_map(|sentence| sentence.get(0)?.as_str())
        .collect::<String>();
    (!translated.is_empty()).then_some(translated)
}

use crate::api::client;
