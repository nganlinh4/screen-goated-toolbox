use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

use crate::api::realtime_audio::qwen3::{Qwen3ModelVariant, assets, reference, runtime, server};

use super::providers;
use super::types::SubtitleGenerationMethod;

static QWEN_LOCAL_PREPARE_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

#[derive(Deserialize)]
struct PrepareQwenLocalArgs {
    #[serde(rename = "subtitleMethod")]
    subtitle_method: SubtitleGenerationMethod,
}

#[derive(Serialize)]
struct PrepareQwenLocalResult {
    available: bool,
    #[serde(rename = "startedDownloads")]
    started_downloads: bool,
    reason: Option<String>,
}

pub fn handle_prepare_qwen_local_subtitles(
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let request: PrepareQwenLocalArgs = serde_json::from_value(args.clone())
        .map_err(|err| format!("Invalid Qwen Local subtitle request: {err}"))?;

    let variant = match request.subtitle_method {
        SubtitleGenerationMethod::QwenLocal0_6B => Qwen3ModelVariant::Small,
        SubtitleGenerationMethod::QwenLocal1_7B => Qwen3ModelVariant::Large,
        _ => {
            return Err(
                "prepare_qwen_local_subtitles only supports Qwen Local methods".to_string(),
            );
        }
    };

    let capability = providers::capabilities()
        .into_iter()
        .find(|entry| entry.method == request.subtitle_method)
        .ok_or_else(|| "Unknown Qwen Local subtitle method".to_string())?;
    if capability.available {
        return serde_json::to_value(PrepareQwenLocalResult {
            available: true,
            started_downloads: false,
            reason: None,
        })
        .map_err(|err| format!("Serialize Qwen Local subtitle preparation: {err}"));
    }

    let installable_from_tools = capability
        .reason
        .as_deref()
        .is_some_and(|reason| reason.contains("Downloaded Tools"));
    if !installable_from_tools {
        return serde_json::to_value(PrepareQwenLocalResult {
            available: false,
            started_downloads: false,
            reason: capability.reason,
        })
        .map_err(|err| format!("Serialize Qwen Local subtitle preparation: {err}"));
    }

    crate::gui::request_open_downloaded_tools();

    let missing_model = match variant {
        Qwen3ModelVariant::Small => !assets::is_qwen3_model_downloaded(),
        Qwen3ModelVariant::Large => !assets::is_qwen3_1_7b_model_downloaded(),
    };
    let missing_runtime = !runtime::has_discoverable_qwen3_runtime();
    let missing_server = !reference::has_discoverable_server();
    let started_downloads = missing_model || missing_runtime || missing_server;

    if started_downloads
        && QWEN_LOCAL_PREPARE_IN_PROGRESS
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    {
        std::thread::spawn(move || {
            let stop_signal = Arc::new(AtomicBool::new(false));
            let _ = (|| -> anyhow::Result<()> {
                match variant {
                    Qwen3ModelVariant::Small => {
                        if missing_model {
                            assets::download_qwen3_model(stop_signal.clone(), true)?;
                        }
                    }
                    Qwen3ModelVariant::Large => {
                        if missing_model {
                            assets::download_qwen3_1_7b_model(stop_signal.clone(), true)?;
                        }
                    }
                }
                if missing_runtime {
                    runtime::download_qwen3_runtime(stop_signal.clone(), true)?;
                }
                if missing_server {
                    server::download_qwen3_server(stop_signal.clone(), true)?;
                }
                Ok(())
            })();
            crate::overlay::auto_copy_badge::hide_progress_notification();
            QWEN_LOCAL_PREPARE_IN_PROGRESS.store(false, Ordering::SeqCst);
        });
    }

    serde_json::to_value(PrepareQwenLocalResult {
        available: false,
        started_downloads,
        reason: capability.reason,
    })
    .map_err(|err| format!("Serialize Qwen Local subtitle preparation: {err}"))
}
