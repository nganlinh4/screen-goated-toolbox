//! Shared microphone policy for the desktop host and recorder worker.
//! Never infer intent from device names or signal levels.

#[cfg(target_os = "windows")]
use cpal::traits::DeviceTrait;
use cpal::traits::HostTrait;

/// Prefer the user's communications endpoint; retain the general default as a
/// fallback when that endpoint is absent or disappears during discovery.
pub(crate) fn microphone_device() -> Option<cpal::Device> {
    let host = cpal::default_host();
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Media::Audio::{eCommunications, eConsole};
        let resolve = |role| {
            let endpoint_id = input_endpoint_id(role)?;
            host.input_devices().ok()?.find(|device| {
                device
                    .id()
                    .ok()
                    .is_some_and(|id| id.id().eq_ignore_ascii_case(&endpoint_id))
            })
        };
        // Resolve both roles to concrete endpoints: the CPAL default proxy can
        // follow a different role later and does not identify the selected mic.
        prefer_communications(resolve(eCommunications), || resolve(eConsole))
    }
    #[cfg(not(target_os = "windows"))]
    host.default_input_device()
}

#[cfg(any(target_os = "windows", test))]
fn prefer_communications<T>(
    communications: Option<T>,
    general: impl FnOnce() -> Option<T>,
) -> Option<T> {
    communications.or_else(general)
}

#[cfg(target_os = "windows")]
fn input_endpoint_id(role: windows::Win32::Media::Audio::ERole) -> Option<String> {
    use windows::Win32::Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator, eCapture};
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
        CoUninitialize,
    };

    unsafe {
        // An existing apartment can also service endpoint discovery. Only balance
        // initialization when this call acquired a COM initialization reference.
        let initialized = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
        let result = (|| {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
            let endpoint = enumerator.GetDefaultAudioEndpoint(eCapture, role).ok()?;
            let pointer = endpoint.GetId().ok()?;
            let id = pointer.to_string().ok();
            CoTaskMemFree(Some(pointer.0.cast()));
            id
        })();
        if initialized {
            CoUninitialize();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn communications_default_wins_without_querying_general_default() {
        assert_eq!(
            prefer_communications(Some("communications"), || panic!("unexpected fallback")),
            Some("communications")
        );
    }

    #[test]
    fn missing_or_disconnected_communications_endpoint_uses_general_default() {
        assert_eq!(
            prefer_communications(None, || Some("general")),
            Some("general")
        );
    }

    #[test]
    fn no_default_does_not_select_an_arbitrary_device() {
        assert_eq!(prefer_communications::<&str>(None, || None), None);
    }

    #[test]
    fn all_microphone_entry_points_share_the_policy() {
        for source in [
            include_str!("api/audio/recording.rs"),
            include_str!("api/audio/gemini_live.rs"),
            include_str!("api/realtime_audio/capture.rs"),
            include_str!("overlay/screen_record/audio_engine/mic_capture.rs"),
        ] {
            assert!(source.contains("crate::audio_input::microphone_device()"));
            assert!(!source.contains(".default_input_device()"));
        }
    }

    #[test]
    fn microphone_consumers_keep_using_the_shared_capture_path() {
        for source in [
            include_str!("overlay/computer_control/runtime/mic.rs"),
            include_str!("overlay/translation_gummy/runtime/mod.rs"),
            include_str!("overlay/tts_playground/runtime_sources.rs"),
            include_str!("api/realtime_audio/transcription.rs"),
            include_str!("api/realtime_audio/parakeet.rs"),
            include_str!("api/realtime_audio/qwen3/mod.rs"),
            include_str!("api/realtime_audio/sherpa_onnx/streaming.rs"),
            include_str!("api/realtime_audio/s2s/live/mod.rs"),
        ] {
            assert!(source.contains("start_mic_capture"));
            assert!(!source.contains(".default_input_device()"));
            assert!(!source.contains(".build_input_stream("));
        }
    }
}
