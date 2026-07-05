use serde_json::{Value, json};

#[cfg(windows)]
use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
#[cfg(windows)]
use windows::Win32::Media::Audio::{
    AudioSessionState, AudioSessionStateActive, AudioSessionStateExpired,
    AudioSessionStateInactive, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
    ISimpleAudioVolume, MMDeviceEnumerator, eMultimedia, eRender,
};
#[cfg(windows)]
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
};
#[cfg(windows)]
use windows_core::Interface;

pub(super) fn active_sessions(args: &Value, observed_at_ms: u128) -> Value {
    #[cfg(not(windows))]
    {
        let _ = args;
        super::failure(
            "audio",
            "active_sessions",
            "audio.active_sessions is only available on Windows",
            observed_at_ms,
        )
    }

    #[cfg(windows)]
    {
        match enumerate_audio_sessions(args) {
            Ok((items, warnings, confidence)) => super::ok(
                "audio",
                "active_sessions",
                "windows_core_audio",
                confidence,
                items,
                warnings,
                observed_at_ms,
            ),
            Err(error) => super::failure(
                "audio",
                "active_sessions",
                &format!("failed to enumerate audio sessions: {error}"),
                observed_at_ms,
            ),
        }
    }
}

#[cfg(windows)]
fn enumerate_audio_sessions(
    args: &Value,
) -> windows::core::Result<(Vec<Value>, Vec<String>, &'static str)> {
    unsafe {
        let include_inactive = args
            .get("include_inactive")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let include_self = args
            .get("include_self")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let current_pid = std::process::id();
        let mut warnings = Vec::new();

        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let device_enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = device_enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
        let session_manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
        let session_enumerator = session_manager.GetSessionEnumerator()?;
        let count = session_enumerator.GetCount()?;
        let mut items = Vec::new();
        let mut peak_meter_count = 0usize;

        for index in 0..count {
            let Ok(session_control) = session_enumerator.GetSession(index) else {
                warnings.push(format!("failed to read audio session at index {index}"));
                continue;
            };
            let state = session_control
                .GetState()
                .unwrap_or(AudioSessionStateInactive);
            if !include_inactive && state != AudioSessionStateActive {
                continue;
            }
            let Ok(session_control2) = session_control.cast::<IAudioSessionControl2>() else {
                warnings.push(format!(
                    "audio session {index} did not expose IAudioSessionControl2"
                ));
                continue;
            };
            let pid = session_control2.GetProcessId().unwrap_or(0);
            if !include_self && pid == current_pid {
                continue;
            }

            let is_system_sounds = session_control2.IsSystemSoundsSession().0 == 0;
            let display_name = session_control
                .GetDisplayName()
                .ok()
                .and_then(pwstr_to_string_and_free);
            let exe_path = if pid > 0 {
                super::process::exe_path(pid)
            } else {
                None
            };
            let process_name = if is_system_sounds {
                "System Sounds".to_string()
            } else {
                exe_path
                    .as_deref()
                    .and_then(super::process::process_name_from_path)
                    .unwrap_or_else(|| format!("PID {pid}"))
            };
            let simple_volume = session_control.cast::<ISimpleAudioVolume>().ok();
            let master_volume = simple_volume
                .as_ref()
                .and_then(|volume| volume.GetMasterVolume().ok());
            let muted = simple_volume
                .as_ref()
                .and_then(|volume| volume.GetMute().ok())
                .map(|value| value.as_bool());
            let peak_value = session_control
                .cast::<IAudioMeterInformation>()
                .ok()
                .and_then(|meter| sample_peak_value(&meter));
            if peak_value.is_some() {
                peak_meter_count += 1;
            }
            let likely_audible = state == AudioSessionStateActive
                && !muted.unwrap_or(false)
                && master_volume.map(|volume| volume > 0.001).unwrap_or(true)
                && peak_value.map(|peak| peak > 0.0005).unwrap_or(true);

            items.push(json!({
                "pid": pid,
                "process_name": process_name,
                "exe_path": exe_path,
                "display_name": display_name,
                "session_state": state_name(state),
                "is_system_sounds": is_system_sounds,
                "is_self": pid == current_pid,
                "master_volume": master_volume,
                "muted": muted,
                "peak_value": peak_value,
                "likely_audible": likely_audible,
                "audibility_basis": if peak_value.is_some() { "session_peak_meter" } else { "state_volume_mute" },
                "evidence": ["core_audio_session", state_name(state)],
            }));
        }

        if items.is_empty() {
            warnings.push("no matching audio sessions on the default render endpoint".to_string());
        } else if peak_meter_count == 0 {
            warnings.push(
                "session peak meters were unavailable; likely_audible uses active state, volume, and mute"
                    .to_string(),
            );
        }

        let confidence = if items.is_empty() || peak_meter_count > 0 {
            "high"
        } else {
            "medium"
        };
        Ok((items, warnings, confidence))
    }
}

#[cfg(windows)]
fn state_name(state: AudioSessionState) -> &'static str {
    if state == AudioSessionStateActive {
        "active"
    } else if state == AudioSessionStateInactive {
        "inactive"
    } else if state == AudioSessionStateExpired {
        "expired"
    } else {
        "unknown"
    }
}

#[cfg(windows)]
fn pwstr_to_string_and_free(value: windows::core::PWSTR) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let string = unsafe { value.to_string().ok().filter(|text| !text.is_empty()) };
    unsafe {
        CoTaskMemFree(Some(value.as_ptr() as *const std::ffi::c_void));
    }
    string
}

#[cfg(windows)]
fn sample_peak_value(meter: &IAudioMeterInformation) -> Option<f32> {
    let mut peak = unsafe { meter.GetPeakValue().ok()? };
    for _ in 0..2 {
        std::thread::sleep(std::time::Duration::from_millis(40));
        if let Ok(next) = unsafe { meter.GetPeakValue() } {
            peak = peak.max(next);
        }
    }
    Some(peak)
}
