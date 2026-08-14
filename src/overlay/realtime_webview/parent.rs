use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{LazyLock, Mutex};

use super::layout::CardRole;
use super::mailbox::{CommandBuffer, PushResult};
use super::protocol::{
    CardSettings, CardText, ChildEvent, DownloadState, HostCommand, RealtimeScene,
};
use super::supervisor::{ProcessState, ensure_process, restart_now, write_command};

const MAX_SCRIPT_BYTES: usize = 128 * 1024;

#[derive(Default)]
struct PendingDelivery {
    commands: CommandBuffer,
    restart: bool,
    warmup: bool,
    snapshot_required: bool,
}

struct DeliveryBatch {
    commands: Vec<HostCommand>,
    restart: bool,
    warmup: bool,
}

pub(super) static SCENE: LazyLock<Mutex<RealtimeScene>> =
    LazyLock::new(|| Mutex::new(initial_scene()));
static PENDING: LazyLock<Mutex<PendingDelivery>> =
    LazyLock::new(|| Mutex::new(PendingDelivery::default()));
static SIGNAL: LazyLock<SyncSender<()>> = LazyLock::new(|| {
    let (sender, receiver) = sync_channel(1);
    std::thread::Builder::new()
        .name("sgt-realtime-delivery".to_string())
        .spawn(move || delivery_loop(receiver))
        .expect("failed to start realtime compositor delivery thread");
    sender
});

fn initial_scene() -> RealtimeScene {
    let settings = current_settings();
    RealtimeScene {
        settings: settings.clone(),
        tts_speed: 100,
        translation_model: settings.translation_model.clone(),
        is_dark: crate::overlay::is_dark_mode(),
        ..Default::default()
    }
}

pub(super) fn warmup() {
    super::supervisor::request_process();
    PENDING.lock().unwrap().warmup = true;
    signal_delivery();
}

pub(super) fn send(command: HostCommand) {
    super::supervisor::request_process();
    let mut pending = PENDING.lock().unwrap();
    if pending.commands.push(command) != PushResult::Queued {
        pending.snapshot_required = true;
    }
    drop(pending);
    signal_delivery();
}

pub(super) fn request_restart() {
    PENDING.lock().unwrap().restart = true;
    signal_delivery();
}

pub(super) fn queue_snapshot() {
    let mut pending = PENDING.lock().unwrap();
    pending
        .commands
        .replace_with_snapshot(HostCommand::Snapshot {
            scene: Box::new(scene_snapshot()),
        });
    pending.snapshot_required = false;
    drop(pending);
    signal_delivery();
}

pub(super) fn scene_snapshot() -> RealtimeScene {
    SCENE.lock().unwrap().clone()
}

pub(super) fn replace_scene(scene: RealtimeScene) {
    *SCENE.lock().unwrap() = scene.clone();
    send(HostCommand::Snapshot {
        scene: Box::new(scene),
    });
}

pub(super) fn set_active(active: bool) {
    let scene = {
        let mut scene = SCENE.lock().unwrap();
        scene.active = active;
        scene.clone()
    };
    send(HostCommand::Snapshot {
        scene: Box::new(scene),
    });
}

pub(super) fn update_text(role: CardRole, text: CardText) {
    {
        let mut scene = SCENE.lock().unwrap();
        match role {
            CardRole::Transcription => scene.transcription.clone_from(&text),
            CardRole::Translation => scene.translation.clone_from(&text),
        }
    }
    send(HostCommand::Text { role, text });
}

pub(super) fn update_settings() {
    let settings = current_settings();
    SCENE.lock().unwrap().settings.clone_from(&settings);
    send(HostCommand::Settings { settings });
}

pub(super) fn update_tts(enabled: bool, speed: u32) {
    {
        let mut scene = SCENE.lock().unwrap();
        scene.tts_enabled = enabled;
        scene.tts_speed = speed;
    }
    send(HostCommand::Tts { enabled, speed });
}

pub(super) fn update_volume(rms: f32) {
    SCENE.lock().unwrap().rms = rms;
    send(HostCommand::Volume { rms });
}

pub(super) fn update_translation_model(model: String) {
    SCENE.lock().unwrap().translation_model.clone_from(&model);
    send(HostCommand::TranslationModel { model });
}

pub(super) fn update_download(state: DownloadState) {
    SCENE.lock().unwrap().download.clone_from(&state);
    send(HostCommand::Download { state });
}

pub(super) fn update_theme(is_dark: bool, font_size: u32) {
    {
        let mut scene = SCENE.lock().unwrap();
        scene.is_dark = is_dark;
        scene.settings.font_size = font_size;
    }
    send(HostCommand::Theme { is_dark, font_size });
}

pub(super) fn run_script(role: Option<CardRole>, script: &str) {
    if script.len() > MAX_SCRIPT_BYTES {
        crate::log_info!(
            "[RealtimeCompositor] rejected oversized script bytes={}",
            script.len()
        );
        return;
    }
    send(HostCommand::Script {
        role,
        script: script.to_string(),
    });
}

fn current_settings() -> CardSettings {
    let config = super::controller::load_session_config();
    CardSettings {
        audio_source: config.audio_source,
        target_language: config.target_language,
        translation_model: config.translation_model,
        transcription_model: config.transcription_model,
        transcription_language: config.transcription_language.to_uppercase(),
        font_size: config.font_size,
    }
}

fn delivery_loop(receiver: Receiver<()>) {
    while receiver.recv().is_ok() {
        while receiver.try_recv().is_ok() {}
        let batch = take_pending();
        let process = if batch.restart {
            restart_now()
        } else if batch.warmup || !batch.commands.is_empty() {
            ensure_process()
        } else {
            ProcessState::Unavailable
        };
        deliver_batch(process, batch.commands);
    }
}

fn deliver_batch(process: ProcessState, commands: Vec<HostCommand>) {
    match process {
        ProcessState::Spawned => {
            if let Err(error) = write_command(&HostCommand::Snapshot {
                scene: Box::new(scene_snapshot()),
            }) {
                crate::log_info!("[RealtimeCompositor] initial snapshot failed: {error:#}");
                super::supervisor::fail_live_renderer("initial snapshot failed", true);
            }
        }
        ProcessState::Unavailable => defer_commands(commands),
        ProcessState::Running => {
            for command in commands {
                if let Err(error) = write_command(&command) {
                    crate::log_info!("[RealtimeCompositor] command delivery failed: {error:#}");
                    super::supervisor::fail_live_renderer("command delivery failed", true);
                    break;
                }
            }
        }
    }
}

fn defer_commands(commands: Vec<HostCommand>) {
    let mut pending = PENDING.lock().unwrap();
    for command in commands {
        if pending.commands.push(command) != PushResult::Queued {
            pending.snapshot_required = true;
        }
    }
}

fn take_pending() -> DeliveryBatch {
    let mut pending = PENDING.lock().unwrap();
    if pending.snapshot_required {
        pending
            .commands
            .replace_with_snapshot(HostCommand::Snapshot {
                scene: Box::new(scene_snapshot()),
            });
        pending.snapshot_required = false;
    }
    DeliveryBatch {
        commands: pending.commands.drain(),
        restart: std::mem::take(&mut pending.restart),
        warmup: std::mem::take(&mut pending.warmup),
    }
}

fn signal_delivery() {
    let _ = SIGNAL.try_send(());
}

pub(super) fn handle_child_event(event: ChildEvent) {
    match event {
        ChildEvent::LayoutChanged { layout } => SCENE.lock().unwrap().layout = layout,
        ChildEvent::Input {
            role,
            body,
            scale: _,
        } => handle_card_input(role, &body),
        ChildEvent::Close => super::manager::request_close_from_renderer(),
        ChildEvent::RendererError { source, error } => {
            crate::log_info!("[RealtimeCompositor] renderer error source={source} error={error}")
        }
        ChildEvent::Ready
        | ChildEvent::Heartbeat
        | ChildEvent::ResyncRequested
        | ChildEvent::RendererFailure { .. } => {}
    }
}

fn handle_card_input(role: CardRole, body: &str) {
    if body == "saveResize" {
        save_size(role);
    } else if let Some(visible) = parse_toggle(body, "toggleMic:") {
        set_visibility(CardRole::Transcription, visible);
    } else if let Some(visible) = parse_toggle(body, "toggleTrans:") {
        set_visibility(CardRole::Translation, visible);
    } else if let Some(size) = body.strip_prefix("fontSize:") {
        if let Ok(size) = size.parse::<u32>() {
            super::controller::set_font_size(size);
            update_settings();
        }
    } else if let Some(source) = body.strip_prefix("audioSource:") {
        super::controller::set_audio_source(source);
        update_settings();
    } else if let Some(language) = body.strip_prefix("language:") {
        super::controller::set_target_language(language);
        update_settings();
    } else if let Some(model) = body.strip_prefix("translationModel:") {
        super::controller::set_translation_model(model);
        update_settings();
    } else if let Some(model) = body.strip_prefix("transcriptionModel:") {
        super::controller::set_transcription_model(model);
        update_settings();
    } else if let Some(language) = body.strip_prefix("transcriptionLanguage:") {
        super::controller::set_transcription_language(language);
        update_settings();
    } else if let Some(enabled) = parse_toggle(body, "ttsEnabled:") {
        super::controller::set_tts_enabled(enabled);
        sync_tts();
        if enabled
            && super::controller::load_session_config().audio_source == "device"
            && !crate::model_config::is_gemini_live_s2s_model_id(
                &super::controller::load_session_config().transcription_model,
            )
        {
            run_script(
                Some(role),
                "if(window.setTtsEnabled)window.setTtsEnabled(false);",
            );
        }
    } else if let Some(speed) = body.strip_prefix("ttsSpeed:") {
        if let Ok(speed) = speed.parse::<u32>() {
            super::controller::set_tts_speed(speed);
            sync_tts();
        }
    } else if let Some(enabled) = parse_toggle(body, "ttsAutoSpeed:") {
        super::controller::set_tts_auto_speed(enabled);
    } else if let Some(volume) = body.strip_prefix("ttsVolume:") {
        if let Ok(volume) = volume.parse::<u32>() {
            super::controller::set_tts_volume(volume);
        }
    } else if body == "cancelDownload" {
        super::controller::cancel_download();
    }
}

fn set_visibility(role: CardRole, visible: bool) {
    use std::sync::atomic::Ordering;
    match role {
        CardRole::Transcription => super::state::MIC_VISIBLE.store(visible, Ordering::SeqCst),
        CardRole::Translation => {
            super::state::TRANS_VISIBLE.store(visible, Ordering::SeqCst);
            if !visible {
                crate::api::tts::TTS_MANAGER.stop();
            }
        }
    }
    if !super::state::MIC_VISIBLE.load(Ordering::SeqCst)
        && !super::state::TRANS_VISIBLE.load(Ordering::SeqCst)
    {
        super::manager::request_close_from_renderer();
    } else if visible {
        super::wndproc::post_text_refresh(role);
    }
}

fn save_size(role: CardRole) {
    let layout = SCENE.lock().unwrap().layout;
    let card = match role {
        CardRole::Transcription => layout.transcription,
        CardRole::Translation => layout.translation,
    };
    if let Ok(mut app) = crate::APP.lock() {
        match role {
            CardRole::Transcription => {
                app.config.realtime_transcription_size = (card.width, card.height)
            }
            CardRole::Translation => {
                app.config.realtime_translation_size = (card.width, card.height)
            }
        }
        crate::config::save_config(&app.config);
    }
}

fn sync_tts() {
    use std::sync::atomic::Ordering;
    update_tts(
        super::state::REALTIME_TTS_ENABLED.load(Ordering::SeqCst),
        super::state::CURRENT_TTS_SPEED.load(Ordering::Relaxed),
    );
}

fn parse_toggle(body: &str, prefix: &str) -> Option<bool> {
    match body.strip_prefix(prefix)? {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::parse_toggle;

    #[test]
    fn card_toggle_payloads_are_structural() {
        assert_eq!(parse_toggle("toggleMic:1", "toggleMic:"), Some(true));
        assert_eq!(parse_toggle("toggleMic:invalid", "toggleMic:"), None);
    }
}
