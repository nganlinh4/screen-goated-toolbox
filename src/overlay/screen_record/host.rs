// Thin host-side launcher for the independently delivered Screen Recorder.

use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use serde::Serialize;
use sgt_recorder_protocol::{Command, VideoDropAction};

pub(crate) mod bg_download;
mod host_launcher;
pub mod mf_audio;
#[path = "mf_decode_min.rs"]
mod mf_decode;
mod thumbnail;

pub(crate) use thumbnail::capture_window_thumbnail;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct AudioDropAction {
    pub path: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct SubtitleDropAction {
    pub path: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct VideoDropActionRecord {
    pub path: String,
    pub action: String,
}

static PENDING_VIDEO_DROPS: LazyLock<Mutex<Vec<VideoDropActionRecord>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static PENDING_AUDIO_DROPS: LazyLock<Mutex<Vec<AudioDropAction>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static PENDING_SUBTITLE_DROPS: LazyLock<Mutex<Vec<SubtitleDropAction>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

pub fn show_screen_record() {
    host_launcher::launch_in_background(Command::Show);
}

pub fn toggle_recording() {
    host_launcher::launch_in_background(Command::Toggle);
}

pub fn queue_video_drop_action(path: String, action: String) {
    if VideoDropAction::parse(&action).is_none() {
        crate::log_info!("[ScreenRecord] ignored unsupported video drop action");
        return;
    }
    PENDING_VIDEO_DROPS
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .push(VideoDropActionRecord { path, action });
}

pub fn queue_audio_drop_action(path: String) {
    PENDING_AUDIO_DROPS
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .push(AudioDropAction { path });
}

pub fn queue_subtitle_drop_action(path: String) {
    PENDING_SUBTITLE_DROPS
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .push(SubtitleDropAction { path });
}

fn pending_commands() -> Vec<Command> {
    let mut commands = Vec::new();
    let videos = std::mem::take(
        &mut *PENDING_VIDEO_DROPS
            .lock()
            .unwrap_or_else(|value| value.into_inner()),
    );
    for drop in videos {
        if let Some(action) = VideoDropAction::parse(&drop.action) {
            commands.push(Command::QueueVideoDrop {
                path: drop.path,
                action,
            });
        }
    }
    let audio = std::mem::take(
        &mut *PENDING_AUDIO_DROPS
            .lock()
            .unwrap_or_else(|value| value.into_inner()),
    );
    commands.extend(
        audio
            .into_iter()
            .map(|drop| Command::QueueAudioDrop { path: drop.path }),
    );
    let subtitles = std::mem::take(
        &mut *PENDING_SUBTITLE_DROPS
            .lock()
            .unwrap_or_else(|value| value.into_inner()),
    );
    commands.extend(
        subtitles
            .into_iter()
            .map(|drop| Command::QueueSubtitleDrop { path: drop.path }),
    );
    commands
}

pub fn post_script(script: String) -> bool {
    host_launcher::send_if_running(Command::EvaluateScript { script }).is_ok()
}

pub fn update_settings() {
    let _ = host_launcher::send_if_running(Command::UpdateSettings);
}

pub fn notify_external_audio_capture_released(reason: &str) {
    let _ = host_launcher::send_if_running(Command::NotifyAudioReleased {
        reason: reason.to_string(),
    });
}

pub fn cleanup_on_app_exit() {
    host_launcher::shutdown();
}

pub(crate) fn stop_for_component_removal() -> anyhow::Result<impl Drop> {
    host_launcher::stop_for_removal()
}

#[cfg(test)]
pub(crate) fn worker_process_is_active() -> bool {
    host_launcher::worker_process_is_active()
}

pub(crate) fn run_gt_narration_test_cli(
    input_wav: &str,
    target_language: &str,
) -> Result<(), String> {
    host_launcher::run_headless(Command::GtNarrationTest {
        input_wav: input_wav.to_string(),
        target_language: target_language.to_string(),
    })
    .map(|_| ())
    .map_err(|error| format!("{error:#}"))
}

pub(crate) fn run_export_replay(
    path: &str,
    runs: u16,
    keep_outputs: bool,
) -> anyhow::Result<serde_json::Value> {
    host_launcher::run_headless(Command::ExportReplay {
        path: path.to_string(),
        runs,
        keep_outputs,
    })
}

pub(crate) fn export_replay_args_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|base| {
        base.join("screen-goated-toolbox")
            .join("export-debug")
            .join("last_export_args.json")
    })
}
