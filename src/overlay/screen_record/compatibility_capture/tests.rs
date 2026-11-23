use super::*;

#[test]
fn desktop_duplication_requires_display_and_a_risky_window() {
    assert!(!route_to_desktop_duplication("monitor", false));
    assert!(route_to_desktop_duplication("monitor", true));
    assert!(!route_to_desktop_duplication("window", false));
    assert!(!route_to_desktop_duplication("window", true));
}

#[test]
fn request_rejects_window_and_requires_a_selected_audio_app() {
    let window = serde_json::json!({"targetType": "window", "targetId": "1"});
    assert!(CaptureRequest::from_args(&window).is_err());
    let per_app = serde_json::json!({"targetType": "monitor", "targetId": "0", "deviceAudioEnabled": true, "deviceAudioMode": "app"});
    assert!(CaptureRequest::from_args(&per_app).is_err());
    let selected_app = serde_json::json!({"targetType": "monitor", "targetId": "0", "deviceAudioEnabled": true, "deviceAudioMode": "app", "deviceAudioAppPid": 1234});
    let request = CaptureRequest::from_args(&selected_app).expect("selected app is valid");
    assert!(matches!(
        request.device_audio_source,
        audio_engine::DeviceAudioCaptureSource::SingleApp(1234)
    ));
}

#[test]
#[ignore = "requires an interactive Windows desktop and FFmpeg"]
fn compatibility_process_lifecycle_smoke() {
    super::super::recording_journal::begin(1).expect("claim live smoke recording");
    let args = serde_json::json!({"targetType": "monitor", "targetId": "0", "fps": 30, "includeCursor": false, "deviceAudioEnabled": true, "deviceAudioMode": "all", "micEnabled": false, "webcamEnabled": false});
    start_inner(&args).expect("start compatibility capture");
    std::thread::sleep(Duration::from_millis(1_200));
    let result = stop_inner().expect("stop compatibility capture");
    let video_path = result["videoFilePath"]
        .as_str()
        .expect("video path in result");
    let audio_path = result["deviceAudioPath"]
        .as_str()
        .expect("device audio path in result");
    assert!(is_nonempty_file(Path::new(video_path)));
    assert!(is_nonempty_file(Path::new(audio_path)));
    super::super::recording_journal::clear().expect("release live smoke ownership");
    eprintln!("[CompatibilityCapture][Smoke] video={video_path} audio={audio_path}");
}

#[test]
#[ignore = "requires an interactive Windows desktop and FFmpeg"]
fn compatibility_abort_stops_process() {
    super::super::recording_journal::begin(1).expect("claim live abort recording");
    let args = serde_json::json!({"targetType": "monitor", "targetId": "0", "fps": 30, "includeCursor": false, "deviceAudioEnabled": false, "micEnabled": false, "webcamEnabled": false});
    start_inner(&args).expect("start compatibility capture");
    std::thread::sleep(Duration::from_millis(300));
    abort();
    assert!(!is_active());
    super::super::recording_journal::clear().expect("release live abort ownership");
}
