use super::*;

fn endpoint_changed(opened: Option<&str>, observed: Option<&str>) -> bool {
    matches!((opened, observed), (Some(a), Some(b)) if !a.eq_ignore_ascii_case(b))
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, PartialEq, Eq)]
enum CommunicationsEndpoint {
    Present(String),
    Absent,
    Unknown,
}

#[cfg(any(target_os = "windows", test))]
fn qualified_observation(
    before: CommunicationsEndpoint,
    selected: Option<String>,
    after: CommunicationsEndpoint,
    console: Option<(CommunicationsEndpoint, CommunicationsEndpoint)>,
) -> Option<String> {
    if before != after {
        return None;
    }
    let selected = selected?;
    match before {
        CommunicationsEndpoint::Present(id) if id.eq_ignore_ascii_case(&selected) => Some(selected),
        CommunicationsEndpoint::Absent => match console {
            Some((CommunicationsEndpoint::Present(a), CommunicationsEndpoint::Present(b)))
                if a.eq_ignore_ascii_case(&b) && a.eq_ignore_ascii_case(&selected) =>
            {
                Some(selected)
            }
            _ => None,
        },
        _ => None,
    }
}

#[cfg(target_os = "windows")]
fn default_endpoint(role: ERole) -> CommunicationsEndpoint {
    use windows::Win32::Foundation::ERROR_NOT_FOUND;
    unsafe {
        let com = CoInitializeEx(None, COINIT_MULTITHREADED);
        let result = (|| {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
            match enumerator.GetDefaultAudioEndpoint(eCapture, role) {
                Ok(device) => Some(match crate::api::audio::endpoint_id::read(&device) {
                    Ok(Some(id)) => CommunicationsEndpoint::Present(id),
                    _ => CommunicationsEndpoint::Unknown,
                }),
                Err(error) if error.code() == ERROR_NOT_FOUND.to_hresult() => {
                    Some(CommunicationsEndpoint::Absent)
                }
                Err(_) => Some(CommunicationsEndpoint::Unknown),
            }
        })()
        .unwrap_or(CommunicationsEndpoint::Unknown);
        if com.is_ok() {
            CoUninitialize();
        }
        result
    }
}

#[cfg(target_os = "windows")]
fn current_selected_endpoint() -> Option<String> {
    let before = default_endpoint(eCommunications);
    let console_before =
        (before == CommunicationsEndpoint::Absent).then(|| default_endpoint(eConsole));
    let selected = concrete_default_input_device()
        .and_then(|device| device.id().ok())
        .map(|id| id.id().to_string());
    let console = console_before.map(|first| (first, default_endpoint(eConsole)));
    qualified_observation(before, selected, default_endpoint(eCommunications), console)
}

#[cfg(not(target_os = "windows"))]
fn current_selected_endpoint() -> Option<String> {
    concrete_default_input_device()
        .and_then(|device| device.id().ok())
        .map(|id| id.id().to_string())
}

fn poll(
    opened: &str,
    stopped: impl Fn() -> bool,
    resolve: impl FnOnce() -> Option<String>,
    reconnect: impl FnOnce(),
) -> bool {
    if stopped() {
        return true;
    }
    let current = resolve();
    if stopped() {
        return true;
    }
    if endpoint_changed(Some(opened), current.as_deref()) {
        reconnect();
        return true;
    }
    false
}

pub(super) fn attach(
    stream: cpal::Stream,
    opened_id: Option<String>,
    stop: Arc<AtomicBool>,
    owner: Option<RealtimeCaptureOwner>,
) -> Result<CaptureStream> {
    let (Some(opened_id), Some(owner)) = (opened_id, owner) else {
        return Ok(CaptureStream::from(stream));
    };
    let monitor = CaptureWorker::spawn("sgt-microphone-device-monitor", move |monitor_stop| {
        loop {
            if poll(
                &opened_id,
                || stop.load(Ordering::Acquire) || monitor_stop.load(Ordering::Acquire),
                current_selected_endpoint,
                || request_device_reconnect("default microphone changed", Some(&owner)),
            ) {
                break;
            }
            std::thread::sleep(Duration::from_millis(750));
        }
    })?;
    Ok(CaptureStream::with_monitor(stream, monitor))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn endpoint_identity_follows_shared_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../parity-fixtures/live-translate/microphone-endpoint.json"
        ))
        .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            assert_eq!(
                endpoint_changed(case["opened"].as_str(), case["observed"].as_str()),
                case["reconnect"].as_bool().unwrap(),
                "{}",
                case["name"],
            );
        }
    }

    #[test]
    fn initial_poll_detects_change_during_capture_open() {
        let calls = Cell::new(0);
        assert!(poll(
            "opened",
            || false,
            || Some("replacement".into()),
            || calls.set(calls.get() + 1)
        ));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn stop_during_lookup_prevents_reconnect() {
        let stopped = Cell::new(false);
        assert!(poll(
            "opened",
            || stopped.get(),
            || {
                stopped.set(true);
                Some("replacement".into())
            },
            || panic!("retired capture must not reconnect")
        ));
    }

    #[test]
    fn transient_lookup_failure_retries() {
        assert!(!poll(
            "opened",
            || false,
            || None,
            || panic!("unknown endpoint")
        ));
    }

    #[test]
    fn console_fallback_requires_confirmed_communications_absence() {
        use CommunicationsEndpoint::{Absent, Present, Unknown};
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../parity-fixtures/live-translate/microphone-endpoint.json"
        ))
        .unwrap();
        for case in fixture["selectionCases"].as_array().unwrap() {
            let endpoint = || match case["communications"].as_str().unwrap() {
                "unknown" => Unknown,
                "absent" => Absent,
                id => Present(id.into()),
            };
            assert_eq!(
                qualified_observation(
                    endpoint(),
                    case["selected"].as_str().map(str::to_owned),
                    endpoint(),
                    Some((Present("console".into()), Present("console".into())))
                )
                .is_some(),
                case["accepted"].as_bool().unwrap(),
                "{}",
                case["name"]
            );
        }
        assert_eq!(
            qualified_observation(Absent, Some("console".into()), Present("comm".into()), None),
            None
        );
        assert_eq!(
            qualified_observation(
                Absent,
                Some("transient-comm".into()),
                Absent,
                Some((Present("console".into()), Present("console".into())))
            ),
            None
        );
    }
}
