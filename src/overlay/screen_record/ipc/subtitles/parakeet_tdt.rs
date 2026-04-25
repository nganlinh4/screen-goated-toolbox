use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

use crate::api::realtime_audio::parakeet_tdt_assets;
use crate::unpack_dlls::{self, AiRuntimeStatus};

use super::providers;
use super::types::SubtitleGenerationMethod;

static PARAKEET_TDT_PREPARE_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

#[derive(Deserialize)]
struct PrepareParakeetTdtArgs {
    #[serde(rename = "subtitleMethod")]
    subtitle_method: SubtitleGenerationMethod,
}

#[derive(Serialize)]
struct PrepareParakeetTdtResult {
    available: bool,
    #[serde(rename = "startedDownloads")]
    started_downloads: bool,
    reason: Option<String>,
}

pub fn handle_prepare_parakeet_tdt_subtitles(
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let request: PrepareParakeetTdtArgs = serde_json::from_value(args.clone())
        .map_err(|err| format!("Invalid Parakeet TDT subtitle request: {err}"))?;
    if request.subtitle_method != SubtitleGenerationMethod::ParakeetTdt0_6BV3 {
        return Err(
            "prepare_parakeet_tdt_subtitles only supports Parakeet TDT methods".to_string(),
        );
    }

    let capability = providers::capabilities()
        .into_iter()
        .find(|entry| entry.method == request.subtitle_method)
        .ok_or_else(|| "Unknown Parakeet TDT subtitle method".to_string())?;
    if capability.available {
        return serde_json::to_value(PrepareParakeetTdtResult {
            available: true,
            started_downloads: false,
            reason: None,
        })
        .map_err(|err| format!("Serialize Parakeet TDT subtitle preparation: {err}"));
    }

    let installable_from_tools = capability
        .reason
        .as_deref()
        .is_some_and(|reason| reason.contains("Downloaded Tools"));
    if !installable_from_tools {
        return serde_json::to_value(PrepareParakeetTdtResult {
            available: false,
            started_downloads: false,
            reason: capability.reason,
        })
        .map_err(|err| format!("Serialize Parakeet TDT subtitle preparation: {err}"));
    }

    crate::gui::request_open_downloaded_tools();

    let missing_model = !parakeet_tdt_assets::is_parakeet_tdt_model_downloaded();
    let missing_runtime = !matches!(
        unpack_dlls::current_ai_runtime_status(),
        AiRuntimeStatus::Installed { .. }
    );
    let started_downloads = missing_model || missing_runtime;

    if started_downloads
        && PARAKEET_TDT_PREPARE_IN_PROGRESS
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    {
        std::thread::spawn(move || {
            let stop_signal = Arc::new(AtomicBool::new(false));
            if missing_model {
                let _ = parakeet_tdt_assets::download_parakeet_tdt_model(stop_signal.clone(), true);
            }
            if missing_runtime {
                let _ = unpack_dlls::start_ai_runtime_install();
            }
            crate::overlay::auto_copy_badge::hide_progress_notification();
            PARAKEET_TDT_PREPARE_IN_PROGRESS.store(false, Ordering::SeqCst);
        });
    }

    serde_json::to_value(PrepareParakeetTdtResult {
        available: false,
        started_downloads,
        reason: capability.reason,
    })
    .map_err(|err| format!("Serialize Parakeet TDT subtitle preparation: {err}"))
}
