// Thin host-side launcher for the independently delivered Screen Recorder.

use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use std::{fmt::Write as _, io::Read as _};

use serde::Serialize;
use sgt_recorder_protocol::{Command, MAX_DECODED_AUDIO_BYTES, VideoDropAction};

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
const MAX_DECODED_WAV_BYTES: u64 = MAX_DECODED_AUDIO_BYTES + 4_096;

struct DecodedAudioOutput(PathBuf);

impl Drop for DecodedAudioOutput {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

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

pub(crate) fn decode_audio_with_optional_worker(path: &std::path::Path) -> Option<Vec<u8>> {
    let input = std::fs::canonicalize(path).ok()?;
    let input_metadata = std::fs::symlink_metadata(&input).ok()?;
    if !input_metadata.is_file() {
        return None;
    }
    let workspace = crate::component_registry::worker_workspace(
        crate::component_registry::recorder::WORKER_ID,
    )
    .ok()?;
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random).ok()?;
    let mut token = String::with_capacity(random.len() * 2);
    for byte in random {
        write!(token, "{byte:02x}").ok()?;
    }
    let output = DecodedAudioOutput(workspace.join(format!("audio-decode-{token}.wav")));
    host_launcher::run_headless(Command::DecodeAudio {
        input_path: input.to_str()?.to_string(),
        output_path: output.0.to_str()?.to_string(),
    })
    .ok()?;

    let metadata = std::fs::symlink_metadata(&output.0).ok()?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_DECODED_WAV_BYTES {
        return None;
    }
    let file = std::fs::File::open(&output.0).ok()?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_DECODED_WAV_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() as u64 == metadata.len()).then_some(bytes)
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
