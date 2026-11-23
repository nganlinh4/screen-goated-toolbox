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
static SCENE_PUBLICATION: Mutex<()> = Mutex::new(());
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
    let _publication = SCENE_PUBLICATION
        .lock()
        .unwrap_or_else(|error| error.into_inner());
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
    publish_scene_change(|current| {
        *current = scene;
        HostCommand::Snapshot {
            scene: Box::new(current.clone()),
        }
    });
}

pub(super) fn set_active(active: bool) {
    publish_scene_change(|scene| {
        scene.active = active;
        HostCommand::Snapshot {
            scene: Box::new(scene.clone()),
        }
    });
}

pub(super) fn update_text(role: CardRole, text: CardText) {
    publish_scene_change(|scene| {
        match role {
            CardRole::Transcription => scene.transcription.clone_from(&text),
            CardRole::Translation => scene.translation.clone_from(&text),
        }
        HostCommand::Text { role, text }
    });
}

/// Brings a card to `committed` and `draft`, sending only what changed.
pub(super) fn publish_text(role: CardRole, committed: &str, draft: &str) {
    publish_ordered(
        &SCENE_PUBLICATION,
        &SCENE,
        |scene| {
            let shown = match role {
                CardRole::Transcription => &mut scene.transcription,
                CardRole::Translation => &mut scene.translation,
            };
            super::text_edit::update(shown, committed, draft)
                .map(|edit| HostCommand::TextEdit { role, edit })
        },
        |command| {
            if let Some(command) = command {
                send(command);
            }
        },
    );
}

pub(super) fn update_settings() {
    let settings = current_settings();
    publish_scene_change(|scene| {
        scene.settings.clone_from(&settings);
        HostCommand::Settings { settings }
    });
}

pub(super) fn update_tts(enabled: bool, speed: u32) {
    publish_scene_change(|scene| {
        scene.tts_enabled = enabled;
        scene.tts_speed = speed;
        HostCommand::Tts { enabled, speed }
    });
}

pub(super) fn update_volume(rms: f32) {
    publish_scene_change(|scene| {
        scene.rms = rms;
        HostCommand::Volume { rms }
    });
}

pub(super) fn update_download(state: DownloadState) {
    publish_scene_change(|scene| {
        scene.download.clone_from(&state);
        HostCommand::Download { state }
    });
}

pub(super) fn update_theme(is_dark: bool, font_size: u32) {
    publish_scene_change(|scene| {
        scene.is_dark = is_dark;
        scene.settings.font_size = font_size;
        HostCommand::Theme { is_dark, font_size }
    });
}

fn publish_scene_change(update: impl FnOnce(&mut RealtimeScene) -> HostCommand) {
    publish_ordered(&SCENE_PUBLICATION, &SCENE, update, send);
}

fn publish_ordered<S, C>(
    publication: &Mutex<()>,
    scene: &Mutex<S>,
    update: impl FnOnce(&mut S) -> C,
    publish: impl FnOnce(C),
) {
    let _publication = publication
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let command = update(&mut scene.lock().unwrap_or_else(|error| error.into_inner()));
    publish(command);
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
            if let Err(error) = write_command(&initial_snapshot()) {
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

/// The first scene of a new renderer. Text edits still queued are already in
/// the scene, and applying one twice would duplicate text, so they are dropped
/// under the publication lock that orders every edit after its scene change.
fn initial_snapshot() -> HostCommand {
    let _publication = SCENE_PUBLICATION
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    PENDING.lock().unwrap().commands.drop_text_edits();
    HostCommand::Snapshot {
        scene: Box::new(scene_snapshot()),
    }
}

/// Puts undelivered commands back ahead of the ones queued since, keeping
/// relative text edits in order.
fn defer_commands(commands: Vec<HostCommand>) {
    let mut pending = PENDING.lock().unwrap();
    let newer = pending.commands.drain();
    for command in commands.into_iter().chain(newer) {
        if pending.commands.push(command) != PushResult::Queued {
            pending.snapshot_required = true;
        }
    }
}

fn take_pending() -> DeliveryBatch {
    // A snapshot taken here must not race a text edit between its scene
    // change and its queueing, so publication is held first (same order as
    // `queue_snapshot`).
    let _publication = SCENE_PUBLICATION
        .lock()
        .unwrap_or_else(|error| error.into_inner());
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

pub(super) fn handle_child_event(event: ChildEvent, session_id: u64, generation: u64) {
    match event {
        ChildEvent::LayoutChanged { layout, .. } => {
            let _publication = SCENE_PUBLICATION
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            SCENE.lock().unwrap().layout = layout;
        }
        ChildEvent::Input { role, body, .. } => {
            handle_card_input(role, &body, session_id, generation)
        }
        ChildEvent::Close { .. } => super::manager::request_close_from_renderer(session_id),
        ChildEvent::RendererError { source, error } => {
            crate::log_info!("[RealtimeCompositor] renderer error source={source} error={error}")
        }
        ChildEvent::Ready
        | ChildEvent::Heartbeat
        | ChildEvent::ResyncRequested
        | ChildEvent::RendererFailure { .. } => {}
    }
}

fn handle_card_input(role: CardRole, body: &str, session_id: u64, generation: u64) {
    if super::smoke::handle_probe(role, body) {
        return;
    }
    if body == "saveResize" {
        save_size(role);
    } else if let Some(visible) = parse_toggle(body, "toggleMic:") {
        set_visibility(CardRole::Transcription, visible, session_id);
    } else if let Some(visible) = parse_toggle(body, "toggleTrans:") {
        set_visibility(CardRole::Translation, visible, session_id);
    } else if let Some(size) = body.strip_prefix("fontSize:") {
        if let Ok(size) = size.parse::<u32>() {
            super::controller::set_font_size(size);
            update_settings();
        }
    } else if let Some(source) = body.strip_prefix("audioSource:") {
        super::controller::set_audio_source(source, session_id, generation);
        update_settings();
    } else if let Some(language) = body.strip_prefix("language:") {
        super::controller::set_target_language(language);
        update_settings();
    } else if let Some(model) = body.strip_prefix("translationModel:") {
        super::controller::set_translation_model(model);
        update_settings();
    } else if let Some(model) = body.strip_prefix("transcriptionModel:") {
        super::controller::set_transcription_model(model, session_id, generation);
        update_settings();
    } else if let Some(language) = body.strip_prefix("transcriptionLanguage:") {
        super::controller::set_transcription_language(language);
        update_settings();
    } else if let Some(encoded) = body.strip_prefix("customVocabulary:") {
        if role == CardRole::Transcription
            && let Ok(decoded) = urlencoding::decode(encoded)
        {
            super::controller::set_custom_vocabulary(&decoded);
        }
    } else if let Some(enabled) = parse_toggle(body, "ttsEnabled:") {
        super::controller::set_tts_enabled(enabled, session_id, generation);
        sync_tts(session_id, generation);
    } else if let Some(speed) = body.strip_prefix("ttsSpeed:") {
        if let Ok(speed) = speed.parse::<u32>() {
            super::controller::set_tts_speed(speed);
            sync_tts(session_id, generation);
        }
    } else if let Some(enabled) = parse_toggle(body, "ttsAutoSpeed:") {
        super::controller::set_tts_auto_speed(enabled);
    } else if let Some(volume) = body.strip_prefix("ttsVolume:") {
        if let Ok(volume) = volume.parse::<u32>() {
            super::controller::set_tts_volume(volume);
        }
    } else if body == "cancelDownload" {
        super::controller::cancel_download_from_renderer(session_id, generation);
    }
}

fn set_visibility(role: CardRole, visible: bool, session_id: u64) {
    use std::sync::atomic::Ordering;
    match role {
        CardRole::Transcription => super::state::MIC_VISIBLE.store(visible, Ordering::SeqCst),
        CardRole::Translation => {
            super::controller::set_translation_visibility(visible);
        }
    }
    if !super::state::MIC_VISIBLE.load(Ordering::SeqCst)
        && !super::state::TRANS_VISIBLE.load(Ordering::SeqCst)
    {
        super::manager::request_close_from_renderer(session_id);
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

fn sync_tts(session_id: u64, generation: u64) {
    use std::sync::atomic::Ordering;
    let stop = super::state::current_stop_signal();
    super::state::publish_if_current(session_id, &stop, || {
        if super::supervisor::is_current_generation(generation) {
            update_tts(
                super::state::REALTIME_TTS_ENABLED.load(Ordering::SeqCst),
                super::state::CURRENT_TTS_SPEED.load(Ordering::Relaxed),
            );
        }
    });
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
    use super::{parse_toggle, publish_ordered};

    #[test]
    fn card_toggle_payloads_are_structural() {
        assert_eq!(parse_toggle("toggleMic:1", "toggleMic:"), Some(true));
        assert_eq!(parse_toggle("toggleMic:invalid", "toggleMic:"), None);
    }

    #[test]
    fn scene_publication_keeps_older_snapshot_ahead_of_newer_snapshot() {
        use std::sync::{Arc, Mutex, mpsc};
        use std::time::Duration;

        let gate = Arc::new(Mutex::new(()));
        let scene = Arc::new(Mutex::new(0));
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (sent_tx, sent_rx) = mpsc::channel();
        let first = {
            let (gate, scene, sent_tx) = (gate.clone(), scene.clone(), sent_tx.clone());
            std::thread::spawn(move || {
                publish_ordered(
                    &gate,
                    &scene,
                    |scene| {
                        *scene = 1;
                        *scene
                    },
                    |snapshot| {
                        entered_tx.send(()).unwrap();
                        release_rx.recv().unwrap();
                        sent_tx.send(snapshot).unwrap();
                    },
                );
            })
        };
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(gate.try_lock().is_err());
        let second = {
            let (gate, scene) = (gate.clone(), scene.clone());
            std::thread::spawn(move || {
                publish_ordered(
                    &gate,
                    &scene,
                    |scene| {
                        *scene = 2;
                        *scene
                    },
                    |snapshot| sent_tx.send(snapshot).unwrap(),
                );
            })
        };
        release_tx.send(()).unwrap();
        first.join().unwrap();
        second.join().unwrap();
        assert_eq!(sent_rx.recv().unwrap(), 1);
        assert_eq!(sent_rx.recv().unwrap(), 2);
        assert_eq!(*scene.lock().unwrap(), 2);
    }
}
