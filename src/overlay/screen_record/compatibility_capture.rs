mod process;
mod supervisor;

use anyhow::{Context, Result, bail};
use parking_lot::Mutex;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::audio_engine;
use super::engine;
use super::engine::types::AUDIO_PATH;
use super::input_capture;
use super::{MEDIA_SERVER_TOKEN, SERVER_PORT};
use process::{CaptureProcessConfig, resolve_ffmpeg};
use supervisor::CaptureSupervisor;

const SIDECAR_STOP_TIMEOUT: Duration = Duration::from_secs(8);

static ACTIVE_SESSION: LazyLock<Mutex<Option<CompatibilitySession>>> =
    LazyLock::new(|| Mutex::new(None));

struct CaptureRequest {
    monitor_index: usize,
    requested_fps: Option<u32>,
    include_cursor: bool,
    device_audio_source: audio_engine::DeviceAudioCaptureSource,
    mic_enabled: bool,
    webcam_enabled: bool,
}

impl CaptureRequest {
    fn from_args(args: &Value) -> Result<Self> {
        let target_type = args["targetType"].as_str().unwrap_or("monitor");
        if target_type != "monitor" {
            bail!("Desktop Duplication compatibility capture supports display capture only");
        }

        let monitor_index = args["targetId"]
            .as_str()
            .or_else(|| args["monitorId"].as_str())
            .unwrap_or("0")
            .parse::<usize>()
            .context("invalid compatibility capture display id")?;
        let requested_fps = args["fps"]
            .as_u64()
            .map(|value| u32::try_from(value).context("capture FPS is too large"))
            .transpose()?;
        if let Some(fps) = requested_fps
            && !(1..=240).contains(&fps)
        {
            bail!("compatibility capture FPS must be between 1 and 240");
        }

        let device_audio_enabled = args["deviceAudioEnabled"].as_bool().unwrap_or(true);
        let device_audio_mode = args["deviceAudioMode"].as_str().unwrap_or("all");
        let device_audio_source = if !device_audio_enabled {
            audio_engine::DeviceAudioCaptureSource::Disabled
        } else if device_audio_mode == "app" {
            let process_id = args["deviceAudioAppPid"]
                .as_u64()
                .context("Select an app before using per-app recording audio")?;
            audio_engine::DeviceAudioCaptureSource::SingleApp(
                u32::try_from(process_id).context("recording audio app PID is too large")?,
            )
        } else {
            audio_engine::DeviceAudioCaptureSource::SystemOutput
        };

        Ok(Self {
            monitor_index,
            requested_fps,
            include_cursor: args["includeCursor"].as_bool().unwrap_or(false),
            device_audio_source,
            mic_enabled: args["micEnabled"].as_bool().unwrap_or(false),
            webcam_enabled: args["webcamEnabled"].as_bool().unwrap_or(true),
        })
    }
}

struct SessionPaths {
    video: PathBuf,
    device_audio: PathBuf,
    mic_audio: PathBuf,
    webcam_video: PathBuf,
}

struct CompatibilitySession {
    supervisor: CaptureSupervisor,
    paths: SessionPaths,
    cursor_stop: Arc<AtomicBool>,
    cursor_thread: Option<JoinHandle<()>>,
    fps: u32,
    started: Instant,
    device_audio_enabled: bool,
}

pub(super) fn is_requested(args: &Value) -> bool {
    let target_type = args["targetType"].as_str().unwrap_or("monitor");
    if target_type != "monitor" {
        return false;
    }
    let target_id = args["targetId"]
        .as_str()
        .or_else(|| args["monitorId"].as_str())
        .unwrap_or("0");
    let monitors = engine::get_monitors();
    let Some(monitor) = monitors.iter().find(|monitor| monitor.id == target_id) else {
        return false;
    };
    let risky_window_detected =
        super::window_capture_eligibility::monitor_requires_desktop_duplication(monitor);
    if risky_window_detected {
        eprintln!(
            "[CaptureBackend] desktop duplication requested reason=fullscreen_presentation_window display={target_id}"
        );
    }
    route_to_desktop_duplication(target_type, risky_window_detected)
}

fn route_to_desktop_duplication(target_type: &str, risky_window_detected: bool) -> bool {
    target_type == "monitor" && risky_window_detected
}

pub(super) fn is_active() -> bool {
    ACTIVE_SESSION.lock().is_some()
}

pub(super) fn start(args: &Value) -> std::result::Result<Value, String> {
    start_inner(args).map_err(|error| format!("{error:#}"))
}

fn start_inner(args: &Value) -> Result<Value> {
    if is_active() {
        bail!("A compatibility capture job is already active");
    }

    let request = CaptureRequest::from_args(args)?;
    let monitors = engine::get_monitors();
    let monitor = monitors
        .get(request.monitor_index)
        .with_context(|| format!("display {} is unavailable", request.monitor_index + 1))?;
    let fps = request
        .requested_fps
        .unwrap_or_else(|| engine::encoder_utils::select_target_fps(monitor.hz));
    let paths = prepare_paths()?;
    let ffmpeg = resolve_ffmpeg().or_else(|resolve_error| {
        eprintln!("[DisplayCapture] FFmpeg unavailable locally: {resolve_error:#}");
        crate::gui::settings_ui::download_manager::ffmpeg_dependency::ensure_ffmpeg_with_badge()
            .map_err(anyhow::Error::msg)
    })?;
    let process_config = CaptureProcessConfig {
        monitor_index: request.monitor_index,
        fps,
        include_cursor: request.include_cursor,
        width: monitor.width,
        height: monitor.height,
        bitrate: engine::encoder_utils::compute_capture_bitrate(monitor.width, monitor.height, fps),
    };

    eprintln!(
        "[DisplayCapture] starting desktop duplication display={} size={}x{} fps={} cursor={} ffmpeg={}",
        request.monitor_index,
        monitor.width,
        monitor.height,
        fps,
        request.include_cursor,
        ffmpeg.display()
    );
    let started = Instant::now();
    let mut supervisor =
        CaptureSupervisor::start(ffmpeg, process_config, paths.video.clone(), started)?;

    initialize_session_state(&paths, monitor, fps);

    let device_audio_enabled = !matches!(
        request.device_audio_source,
        audio_engine::DeviceAudioCaptureSource::Disabled
    );
    if device_audio_enabled {
        let audio_result = match request.device_audio_source {
            audio_engine::DeviceAudioCaptureSource::Disabled => unreachable!(),
            audio_engine::DeviceAudioCaptureSource::SystemOutput => {
                audio_engine::record_device_audio_sidecar(
                    paths.device_audio.to_string_lossy().to_string(),
                    started,
                    engine::SHOULD_STOP_AUDIO.clone(),
                    engine::AUDIO_ENCODING_FINISHED.clone(),
                )
            }
            audio_engine::DeviceAudioCaptureSource::SingleApp(process_id) => {
                audio_engine::record_app_audio_sidecar(
                    paths.device_audio.to_string_lossy().to_string(),
                    started,
                    engine::SHOULD_STOP_AUDIO.clone(),
                    engine::AUDIO_ENCODING_FINISHED.clone(),
                    process_id,
                )
            }
        };
        if let Err(error) = audio_result {
            engine::AUDIO_ENCODING_FINISHED.store(true, Ordering::SeqCst);
            reset_failed_start();
            supervisor.abort();
            let _ = std::fs::remove_file(&paths.video);
            return Err(error.context("start display capture audio"));
        }
        *AUDIO_PATH.lock().unwrap() = Some(paths.device_audio.to_string_lossy().to_string());
    } else {
        engine::AUDIO_ENCODING_FINISHED.store(true, Ordering::SeqCst);
        *AUDIO_PATH.lock().unwrap() = None;
    }

    start_optional_sidecars(&request, &paths, started);
    let cursor_stop = Arc::new(AtomicBool::new(false));
    let cursor_thread = Some(engine::spawn_cursor_sampler(
        started,
        cursor_stop.clone(),
        engine::compute_cursor_sample_interval(fps),
    ));
    if let Err(error) = input_capture::start_capture() {
        eprintln!("[CompatibilityCapture] input capture start failed: {error}");
    }

    engine::IS_RECORDING.store(true, Ordering::SeqCst);
    *ACTIVE_SESSION.lock() = Some(CompatibilitySession {
        supervisor,
        paths,
        cursor_stop,
        cursor_thread,
        fps,
        started,
        device_audio_enabled,
    });

    println!(
        "[CaptureBackend] selected=desktop-duplication-process reason=fullscreen_presentation_window"
    );
    Ok(Value::Null)
}

pub(super) fn stop() -> std::result::Result<Value, String> {
    stop_inner().map_err(|error| format!("{error:#}"))
}

fn stop_inner() -> Result<Value> {
    let mut session = ACTIVE_SESSION
        .lock()
        .take()
        .context("No compatibility capture job is active")?;
    eprintln!(
        "[CompatibilityCapture][Stop] requested elapsed_ms={}",
        session.started.elapsed().as_millis()
    );
    let recording_duration = session.started.elapsed();

    engine::SHOULD_STOP.store(true, Ordering::SeqCst);
    engine::SHOULD_STOP_AUDIO.store(true, Ordering::SeqCst);
    session.cursor_stop.store(true, Ordering::SeqCst);
    if let Some(thread) = session.cursor_thread.take() {
        let _ = thread.join();
    }
    let input_events = input_capture::stop_capture_and_drain();

    let capture_result = session.supervisor.stop(recording_duration);
    engine::ENCODING_FINISHED.store(true, Ordering::SeqCst);
    wait_for_sidecars();

    let segment_count = capture_result.inspect_err(|_| reset_after_stop())?;
    if !is_nonempty_file(&session.paths.video) {
        reset_after_stop();
        bail!("Compatibility capture produced no video");
    }

    let result = build_result(&session, input_events)?;
    reset_after_stop();
    eprintln!(
        "[CompatibilityCapture][Stop] complete elapsed_ms={} segments={} video_path={}",
        session.started.elapsed().as_millis(),
        segment_count,
        session.paths.video.display()
    );
    Ok(result)
}

pub(super) fn abort() {
    let Some(mut session) = ACTIVE_SESSION.lock().take() else {
        return;
    };
    eprintln!("[CompatibilityCapture] aborting active process capture");
    engine::SHOULD_STOP.store(true, Ordering::SeqCst);
    engine::SHOULD_STOP_AUDIO.store(true, Ordering::SeqCst);
    session.cursor_stop.store(true, Ordering::SeqCst);
    if let Some(thread) = session.cursor_thread.take() {
        let _ = thread.join();
    }
    session.supervisor.abort();
    input_capture::stop_capture_and_drain();
    reset_after_stop();
}

fn prepare_paths() -> Result<SessionPaths> {
    let directory = crate::paths::app_local_data_dir().join("recordings");
    std::fs::create_dir_all(&directory)
        .with_context(|| format!("create recordings directory {}", directory.display()))?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock precedes Unix epoch")?
        .as_millis();
    let stem = format!("recording_{timestamp}_display");
    Ok(SessionPaths {
        video: directory.join(format!("{stem}.mp4")),
        device_audio: directory.join(format!("{stem}_device.wav")),
        mic_audio: directory.join(format!("{stem}_mic.wav")),
        webcam_video: directory.join(format!("{stem}_webcam.mp4")),
    })
}

fn initialize_session_state(paths: &SessionPaths, monitor: &engine::MonitorInfo, fps: u32) {
    engine::SHOULD_STOP.store(false, Ordering::SeqCst);
    engine::SHOULD_STOP_AUDIO.store(false, Ordering::SeqCst);
    engine::ENCODING_FINISHED.store(false, Ordering::SeqCst);
    engine::AUDIO_ENCODING_FINISHED.store(false, Ordering::SeqCst);
    engine::MIC_AUDIO_ENCODING_FINISHED.store(true, Ordering::SeqCst);
    engine::WEBCAM_ENCODING_FINISHED.store(true, Ordering::SeqCst);
    engine::ENCODER_ACTIVE.store(false, Ordering::SeqCst);
    *engine::CAPTURE_ERROR.lock() = None;
    engine::MOUSE_POSITIONS.lock().clear();
    engine::reset_cursor_detection_state();
    engine::TARGET_HWND.store(0, Ordering::SeqCst);
    engine::LAST_CAPTURE_FRAME_WIDTH.store(monitor.width as usize, Ordering::SeqCst);
    engine::LAST_CAPTURE_FRAME_HEIGHT.store(monitor.height as usize, Ordering::SeqCst);
    *engine::LAST_RECORDING_FPS.lock().unwrap() = Some(fps);
    *engine::VIDEO_PATH.lock().unwrap() = Some(paths.video.to_string_lossy().to_string());
    *engine::MIC_AUDIO_PATH.lock().unwrap() = None;
    *engine::WEBCAM_VIDEO_PATH.lock().unwrap() = None;
    engine::MIC_AUDIO_START_OFFSET_MS.store(u64::MAX, Ordering::SeqCst);
    engine::WEBCAM_VIDEO_START_OFFSET_MS.store(u64::MAX, Ordering::SeqCst);
    unsafe {
        engine::MONITOR_X = monitor.x;
        engine::MONITOR_Y = monitor.y;
    }
}

fn start_optional_sidecars(request: &CaptureRequest, paths: &SessionPaths, started: Instant) {
    if request.mic_enabled {
        engine::MIC_AUDIO_ENCODING_FINISHED.store(false, Ordering::SeqCst);
        match audio_engine::record_mic_audio_sidecar(
            paths.mic_audio.to_string_lossy().to_string(),
            started,
            engine::SHOULD_STOP_AUDIO.clone(),
            engine::MIC_AUDIO_ENCODING_FINISHED.clone(),
            &engine::MIC_AUDIO_START_OFFSET_MS,
        ) {
            Ok(()) => {
                *engine::MIC_AUDIO_PATH.lock().unwrap() =
                    Some(paths.mic_audio.to_string_lossy().to_string());
            }
            Err(error) => {
                engine::MIC_AUDIO_ENCODING_FINISHED.store(true, Ordering::SeqCst);
                eprintln!("[CompatibilityCapture] microphone unavailable: {error}");
            }
        }
    }

    if request.webcam_enabled {
        engine::WEBCAM_ENCODING_FINISHED.store(false, Ordering::SeqCst);
        match super::webcam_capture::record_webcam_video_sidecar(
            paths.webcam_video.to_string_lossy().to_string(),
            started,
            engine::SHOULD_STOP_AUDIO.clone(),
            engine::WEBCAM_ENCODING_FINISHED.clone(),
            &engine::WEBCAM_VIDEO_START_OFFSET_MS,
        ) {
            Ok(()) => {
                *engine::WEBCAM_VIDEO_PATH.lock().unwrap() =
                    Some(paths.webcam_video.to_string_lossy().to_string());
            }
            Err(error) => {
                engine::WEBCAM_ENCODING_FINISHED.store(true, Ordering::SeqCst);
                eprintln!("[CompatibilityCapture] webcam unavailable: {error}");
            }
        }
    }
}

fn wait_for_sidecars() {
    let deadline = Instant::now() + SIDECAR_STOP_TIMEOUT;
    while Instant::now() < deadline
        && (!engine::AUDIO_ENCODING_FINISHED.load(Ordering::SeqCst)
            || !engine::MIC_AUDIO_ENCODING_FINISHED.load(Ordering::SeqCst)
            || !engine::WEBCAM_ENCODING_FINISHED.load(Ordering::SeqCst))
    {
        std::thread::sleep(Duration::from_millis(25));
    }
    eprintln!(
        "[CompatibilityCapture][Stop] sidecars audio={} mic={} webcam={}",
        engine::AUDIO_ENCODING_FINISHED.load(Ordering::SeqCst),
        engine::MIC_AUDIO_ENCODING_FINISHED.load(Ordering::SeqCst),
        engine::WEBCAM_ENCODING_FINISHED.load(Ordering::SeqCst)
    );
}

fn build_result(
    session: &CompatibilitySession,
    input_events: Vec<input_capture::RawInputEvent>,
) -> Result<Value> {
    let mut port = SERVER_PORT.load(Ordering::SeqCst);
    if port == 0 {
        port = super::ipc::start_global_media_server().unwrap_or(0);
    }
    let token = MEDIA_SERVER_TOKEN.get().cloned().unwrap_or_default();
    let video_path = session.paths.video.to_string_lossy().to_string();
    let device_audio_path = session
        .device_audio_enabled
        .then_some(&session.paths.device_audio)
        .filter(|path| is_nonempty_file(path))
        .map(|path| path.to_string_lossy().to_string());
    let mic_audio_path = is_nonempty_file(&session.paths.mic_audio)
        .then(|| session.paths.mic_audio.to_string_lossy().to_string());
    let webcam_video_path = is_nonempty_file(&session.paths.webcam_video)
        .then(|| session.paths.webcam_video.to_string_lossy().to_string());
    let mouse_positions = engine::MOUSE_POSITIONS.lock().drain(..).collect::<Vec<_>>();
    let mic_offset = offset_seconds(&engine::MIC_AUDIO_START_OFFSET_MS);
    let webcam_offset = offset_seconds(&engine::WEBCAM_VIDEO_START_OFFSET_MS);

    Ok(serde_json::json!({
        "videoUrl": media_url(port, &token, &video_path),
        "deviceAudioUrl": device_audio_path
            .as_deref()
            .map(|path| media_url(port, &token, path))
            .unwrap_or_default(),
        "micAudioUrl": mic_audio_path
            .as_deref()
            .map(|path| media_url(port, &token, path))
            .unwrap_or_default(),
        "webcamVideoUrl": webcam_video_path
            .as_deref()
            .map(|path| media_url(port, &token, path))
            .unwrap_or_default(),
        "mouseData": mouse_positions,
        "deviceAudioPath": device_audio_path.unwrap_or_default(),
        "micAudioPath": mic_audio_path.unwrap_or_default(),
        "webcamVideoPath": webcam_video_path.unwrap_or_default(),
        "micAudioOffsetSec": mic_offset,
        "webcamVideoOffsetSec": webcam_offset,
        "videoFilePath": video_path,
        "inputEvents": input_events,
        "capturedFps": session.fps
    }))
}

fn media_url(port: u16, token: &str, path: &str) -> String {
    format!(
        "http://localhost:{}/?path={}&token={}",
        port,
        urlencoding::encode(path),
        urlencoding::encode(token)
    )
}

fn offset_seconds(offset: &std::sync::atomic::AtomicU64) -> f64 {
    match offset.load(Ordering::SeqCst) {
        u64::MAX => 0.0,
        value => value as f64 / 1000.0,
    }
}

fn is_nonempty_file(path: &Path) -> bool {
    path.metadata()
        .map(|metadata| metadata.len() > 0)
        .unwrap_or(false)
}

fn reset_failed_start() {
    engine::SHOULD_STOP_AUDIO.store(true, Ordering::SeqCst);
    reset_after_stop();
}

fn reset_after_stop() {
    engine::IS_RECORDING.store(false, Ordering::SeqCst);
    engine::ENCODER_ACTIVE.store(false, Ordering::SeqCst);
    engine::ENCODING_FINISHED.store(true, Ordering::SeqCst);
    engine::AUDIO_ENCODING_FINISHED.store(true, Ordering::SeqCst);
    engine::MIC_AUDIO_ENCODING_FINISHED.store(true, Ordering::SeqCst);
    engine::WEBCAM_ENCODING_FINISHED.store(true, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
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
        let window = serde_json::json!({
            "targetType": "window",
            "targetId": "1"
        });
        assert!(CaptureRequest::from_args(&window).is_err());

        let per_app = serde_json::json!({
            "targetType": "monitor",
            "targetId": "0",
            "deviceAudioEnabled": true,
            "deviceAudioMode": "app"
        });
        assert!(CaptureRequest::from_args(&per_app).is_err());

        let selected_app = serde_json::json!({
            "targetType": "monitor",
            "targetId": "0",
            "deviceAudioEnabled": true,
            "deviceAudioMode": "app",
            "deviceAudioAppPid": 1234
        });
        let request = CaptureRequest::from_args(&selected_app).expect("selected app is valid");
        assert!(matches!(
            request.device_audio_source,
            audio_engine::DeviceAudioCaptureSource::SingleApp(1234)
        ));
    }

    #[test]
    #[ignore = "requires an interactive Windows desktop and FFmpeg"]
    fn compatibility_process_lifecycle_smoke() {
        let args = serde_json::json!({
            "targetType": "monitor",
            "targetId": "0",
            "fps": 30,
            "includeCursor": false,
            "deviceAudioEnabled": true,
            "deviceAudioMode": "all",
            "micEnabled": false,
            "webcamEnabled": false
        });
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
        eprintln!("[CompatibilityCapture][Smoke] video={video_path} audio={audio_path}");
    }

    #[test]
    #[ignore = "requires an interactive Windows desktop and FFmpeg"]
    fn compatibility_abort_stops_process() {
        let args = serde_json::json!({
            "targetType": "monitor",
            "targetId": "0",
            "fps": 30,
            "includeCursor": false,
            "deviceAudioEnabled": false,
            "micEnabled": false,
            "webcamEnabled": false
        });
        start_inner(&args).expect("start compatibility capture");
        std::thread::sleep(Duration::from_millis(300));
        abort();
        assert!(!is_active());
    }
}
