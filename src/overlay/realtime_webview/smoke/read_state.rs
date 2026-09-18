//! Read-state acceptance against the actual compositor card DOM.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::overlay::realtime_webview::layout::CardRole;
use crate::overlay::realtime_webview::{controller, parent, state, supervisor, wndproc};

static ACTIVE: AtomicBool = AtomicBool::new(false);
static NEXT_PROBE: AtomicU64 = AtomicU64::new(1);
static RESPONSE: Mutex<Option<ReadProbe>> = Mutex::new(None);

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReadProbe {
    id: u64,
    enabled: bool,
    toggle_enabled: bool,
    locked: bool,
}

pub(in crate::overlay::realtime_webview) fn handle_probe(role: CardRole, body: &str) -> bool {
    if !ACTIVE.load(Ordering::SeqCst) || role != CardRole::Translation {
        return false;
    }
    let Some(payload) = body.strip_prefix("readStateProbe:") else {
        return false;
    };
    if let Ok(probe) = serde_json::from_str(payload) {
        *RESPONSE.lock().unwrap() = Some(probe);
    }
    true
}

pub(super) fn verify() -> bool {
    ACTIVE.store(true, Ordering::SeqCst);
    let result = verify_transitions();
    ACTIVE.store(false, Ordering::SeqCst);
    if let Err(error) = &result {
        crate::log_info!("[RealtimeReadSmoke] failed error={error:#}");
    }
    result.is_ok()
}

fn verify_transitions() -> anyhow::Result<()> {
    let mut scene = parent::scene_snapshot();
    scene.layout.translation = scene.layout.transcription;
    scene.layout.translation.y += scene.layout.transcription.height + 16;
    scene.layout.translation.visible = true;
    scene.settings.audio_source = "mic".into();
    let model_for_mode = |direct_speech: bool| {
        crate::model_config::realtime_transcription_model_options()
            .iter()
            .map(|(id, _)| *id)
            .find(|id| crate::model_config::is_gemini_live_s2s_model_id(id) == direct_speech)
            .ok_or_else(|| {
                anyhow::anyhow!("missing realtime model for direct_speech={direct_speech}")
            })
    };
    let direct = model_for_mode(true)?;
    state::REALTIME_TTS_ENABLED.store(false, Ordering::SeqCst);
    scene.tts_enabled = controller::reconcile_read_model(direct);
    scene.settings.transcription_model = direct.into();
    parent::replace_scene(scene.clone());
    expect_probe("direct-start", true, true)?;

    let transcription = model_for_mode(false)?;
    scene.tts_enabled = controller::reconcile_read_model(transcription);
    scene.settings.transcription_model = transcription.into();
    parent::replace_scene(scene);
    expect_probe("transcription-switch", true, false)?;

    unsafe {
        wndproc::realtime_wnd_proc(
            windows::Win32::Foundation::HWND::default(),
            crate::api::realtime_audio::WM_REALTIME_UPDATE,
            windows::Win32::Foundation::WPARAM(0),
            windows::Win32::Foundation::LPARAM(0),
        );
    }
    expect_probe("first-transcript-refresh", true, false)?;
    anyhow::ensure!(
        supervisor::restart_and_wait(Duration::from_secs(10)),
        "renderer restart failed"
    );
    expect_probe("renderer-restart", true, false)
}

fn expect_probe(phase: &str, enabled: bool, locked: bool) -> anyhow::Result<()> {
    let id = NEXT_PROBE.fetch_add(1, Ordering::SeqCst);
    *RESPONSE.lock().unwrap() = None;
    parent::run_script(
        Some(CardRole::Translation),
        &format!(
            "window.realtimePostMessage('readStateProbe:'+JSON.stringify({{id:{id},enabled:document.getElementById('speak-btn').classList.contains('active'),toggleEnabled:document.getElementById('tts-toggle').classList.contains('on'),locked:document.getElementById('speak-btn').classList.contains('locked')}}));"
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
            crate::log_info!(
                "[RealtimeReadSmoke] phase={} enabled={} toggle_enabled={} locked={}",
                phase,
                probe.enabled,
                probe.toggle_enabled,
                probe.locked
            );
            anyhow::ensure!(
                probe.enabled == enabled
                    && probe.toggle_enabled == enabled
                    && probe.locked == locked,
                "Read DOM state mismatch at {phase}"
            );
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    anyhow::bail!("Read DOM probe timed out at {phase}")
}
