//! First-use acceptance through the normal session manager and renderer.

use anyhow::{Context, Result, ensure};
use std::os::windows::ffi::OsStrExt as _;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FILENAME, SND_NODEFAULT, SND_SYNC};
use windows::core::PCWSTR;

use crate::overlay::realtime_webview::layout::CardRole;
use crate::overlay::realtime_webview::{manager, parent, state, supervisor};

static ACTIVE: AtomicBool = AtomicBool::new(false);
static NEXT_PROBE: AtomicU64 = AtomicU64::new(1);
static RESPONSE: Mutex<Option<Probe>> = Mutex::new(None);

#[derive(serde::Deserialize)]
struct Probe {
    id: u64,
    visible: bool,
    progress: f32,
    selected: bool,
}

pub(in crate::overlay::realtime_webview) fn handle_probe(role: CardRole, body: &str) -> bool {
    if !ACTIVE.load(Ordering::SeqCst) || role != CardRole::Transcription {
        return false;
    }
    let Some(payload) = body.strip_prefix("firstUseProbe:") else {
        return false;
    };
    if let Ok(probe) = serde_json::from_str(payload) {
        *RESPONSE.lock().unwrap() = Some(probe);
    }
    true
}

pub(crate) fn run(wav_path: &Path, require_download: bool) -> i32 {
    let label = if require_download {
        "RealtimeFirstUseSmoke"
    } else {
        "RealtimeContinuationSmoke"
    };
    ACTIVE.store(true, Ordering::SeqCst);
    let result = verify(wav_path, require_download, label);
    // Stop without invoking the download-cancel UI action, which persists a
    // replacement selection. This harness never writes the user's configuration.
    state::set_current_stop_signal(true);
    crate::api::tts::TTS_MANAGER.stop();
    parent::set_active(false);
    ACTIVE.store(false, Ordering::SeqCst);
    match result {
        Ok(()) => {
            crate::log_info!("[{label}] status=passed");
            0
        }
        Err(error) => {
            crate::log_info!("[{label}] status=failed error={error:#}");
            1
        }
    }
}

fn verify(wav_path: &Path, require_download: bool, label: &str) -> Result<()> {
    ensure!(
        std::env::var_os("SGT_RUNTIME_STATE_ROOT")
            .is_some_and(|root| !root.is_empty() && Path::new(&root).is_absolute()),
        "an isolated runtime-state root is required"
    );
    let mut reader = hound::WavReader::open(wav_path).context("open public speech fixture")?;
    let spec = reader.spec();
    ensure!(
        spec.channels == 1
            && spec.sample_rate == 24_000
            && spec.bits_per_sample == 16
            && spec.sample_format == hound::SampleFormat::Int,
        "fixture must be mono 24kHz 16-bit PCM"
    );
    ensure!(
        reader.duration() <= 24_000 * 30,
        "fixture exceeds 30 seconds"
    );
    let samples = reader
        .samples::<i16>()
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(
        !samples.is_empty() && samples.len() <= 24_000 * 30,
        "fixture must be 0–30 seconds"
    );
    ensure!(
        crate::api::realtime_audio::model_loader::is_model_downloaded() != require_download,
        "model presence must match first-use/continuation mode; existing models are never removed"
    );
    ensure!(
        supervisor::wait_until_ready(Duration::from_secs(10)),
        "renderer not ready"
    );
    {
        let mut app = crate::APP.lock().unwrap();
        app.config.realtime_transcription_model = "parakeet".into();
        app.config.realtime_audio_source = "device".into();
        app.config.realtime_target_language = "English".into();
        app.config.realtime_translation_model = "google-gtx".into();
    }
    let playback = FixturePlayback(wav_path.as_os_str().encode_wide().chain(Some(0)).collect());
    manager::show_realtime_overlay();
    let started = Instant::now();
    let mut saw_download = false;
    let mut saw_progress = false;
    let mut local_only = false;
    let mut played = false;
    let mut playback_started = None;
    let mut last_play = Instant::now();
    let mut last_report = Instant::now();
    while started.elapsed() < Duration::from_secs(900) {
        let (downloading, progress, transcript) = {
            let current = state::REALTIME_STATE.lock().unwrap();
            (
                current.is_downloading,
                current.download_progress,
                current.display_transcript.clone(),
            )
        };
        ensure!(
            !transcript.contains("[Error:"),
            "normal transcription path reported an error: {transcript}"
        );
        let scene = parent::scene_snapshot();
        if scene.active && !local_only {
            // Keep this local-ASR check local. The normal translation loop
            // respects this visibility state before any provider request.
            state::TRANS_VISIBLE.store(false, Ordering::SeqCst);
            let mut local_scene = scene.clone();
            local_scene.layout.translation.visible = false;
            parent::replace_scene(local_scene);
            local_only = true;
        }
        if downloading && scene.download.active {
            saw_download = true;
            if progress > 1.0 && progress < 99.0 && !saw_progress {
                let probe = probe()?;
                crate::log_info!(
                    "[{label}] phase=download_dom visible={} selected={} progress={}",
                    probe.visible,
                    probe.selected,
                    probe.progress
                );
                ensure!(
                    probe.visible && probe.selected && probe.progress > 0.0,
                    "download progress is not visible in selected model's DOM"
                );
                saw_progress = true;
                crate::log_info!(
                    "[{label}] phase=visible_progress progress={}",
                    probe.progress
                );
            }
        }
        let install_complete =
            !require_download || (saw_download && saw_progress && progress >= 100.0);
        if install_complete && scene.active && !downloading && !scene.download.active {
            if !played {
                let probe = probe()?;
                ensure!(
                    !probe.visible && probe.selected,
                    "download modal did not close with selection preserved"
                );
                crate::log_info!(
                    "[{label}] phase=ready_to_continue elapsed_ms={}",
                    started.elapsed().as_millis()
                );
                // Give the verified worker and capture device time to start.
                std::thread::sleep(Duration::from_secs(4));
                playback_started = Some(Instant::now());
            }
            ensure!(
                playback_started
                    .is_none_or(|started: Instant| started.elapsed() < Duration::from_secs(60)),
                "installed model did not transcribe the public speech within 60 seconds"
            );
            if !played || last_play.elapsed() > Duration::from_secs(12) {
                playback.play()?;
                played = true;
                last_play = Instant::now();
            }
            if played && transcript.to_lowercase().contains("public demonstration") {
                let model_dir = crate::api::realtime_audio::model_loader::get_parakeet_model_dir();
                ensure!(
                    crate::api::realtime_audio::model_loader::parakeet_model_contracts()
                        .iter()
                        .all(|contract| {
                            crate::api::realtime_audio::model_loader::verified_file_present(
                                &model_dir.join(contract.name),
                                *contract,
                            )
                        }),
                    "installed model failed verification"
                );
                crate::log_info!(
                    "[{label}] phase=transcription_received chars={} elapsed_ms={}",
                    transcript.chars().count(),
                    started.elapsed().as_millis()
                );
                return Ok(());
            }
        }
        if last_report.elapsed() > Duration::from_secs(10) {
            crate::log_info!(
                "[{label}] phase=waiting downloading={downloading} progress={progress} elapsed_ms={}",
                started.elapsed().as_millis()
            );
            last_report = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    anyhow::bail!("first-use did not reach automatic transcription before deadline")
}

// Fixture audio is loopback input, not TTS output: the latter deliberately
// triggers the product's echo suppression. Drop always stops native playback.
struct FixturePlayback(Vec<u16>);

impl FixturePlayback {
    fn play(&self) -> Result<()> {
        ensure!(
            unsafe {
                PlaySoundW(
                    PCWSTR(self.0.as_ptr()),
                    None,
                    SND_FILENAME | SND_ASYNC | SND_NODEFAULT,
                )
                .as_bool()
            },
            "public fixture playback failed"
        );
        Ok(())
    }
}

impl Drop for FixturePlayback {
    fn drop(&mut self) {
        unsafe {
            let _ = PlaySoundW(PCWSTR::null(), None, SND_SYNC);
        }
    }
}

fn probe() -> Result<Probe> {
    let id = NEXT_PROBE.fetch_add(1, Ordering::SeqCst);
    *RESPONSE.lock().unwrap() = None;
    parent::run_script(
        Some(CardRole::Transcription),
        &format!(
            "window.realtimePostMessage('firstUseProbe:'+JSON.stringify({{id:{id},visible:document.getElementById('download-modal').classList.contains('show'),progress:parseFloat(document.getElementById('download-fill').style.width)||0,selected:document.getElementById('transcription-model-select').value==='parakeet'}}));"
        ),
    );
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        if let Some(probe) = RESPONSE
            .lock()
            .unwrap()
            .take()
            .filter(|probe| probe.id == id)
        {
            return Ok(probe);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    anyhow::bail!("download DOM probe timed out")
}
