//! Step Audio EditX offline TTS worker.
//!
//! Step Audio runs through a managed persistent Python sidecar, not a libtorch
//! DLL. The sidecar owns PyTorch, the bundled upstream Step-Audio-EditX source,
//! and the loaded model so requests do not pay cold-start cost repeatedly.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};
use std::{collections::VecDeque, fmt::Write as _};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use super::super::manager::TtsManager;
use super::super::types::{AudioEvent, QueuedRequest};
use super::open_weights::{fail_request, stream_pcm_samples};
use crate::api::realtime_audio::step_audio_assets::{
    download_step_audio_model, get_step_audio_editx_dir, get_step_audio_tokenizer_dir,
    is_step_audio_model_downloaded, is_step_audio_model_downloading,
};
use crate::api::realtime_audio::step_audio_runtime::{
    download_step_audio_runtime, get_step_audio_runtime_entrypoint,
    is_step_audio_runtime_downloading, is_step_audio_runtime_installed,
};
use crate::config::step_audio_tts_text_issue;

const PROVIDER: &str = "StepAudio";
const STEP_AUDIO_TIMEOUT: Duration = Duration::from_secs(900);
const STEP_AUDIO_CANCEL_POLL: Duration = Duration::from_millis(200);
const STEP_AUDIO_STDERR_TAIL_LINES: usize = 80;

lazy_static::lazy_static! {
    static ref STEP_AUDIO_SIDECAR: Mutex<Option<StepAudioSidecarClient>> = Mutex::new(None);
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StepAudioSidecarRequest {
    id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    operation: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    text: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    voice: String,
    step_model_dir: String,
    tokenizer_dir: String,
    output_wav_path: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    prompt_audio_path: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    prompt_text: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    source_audio_path: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    source_text: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    edit_type: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    edit_info: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    target_text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StepAudioSidecarResponse {
    #[serde(default)]
    id: String,
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    sample_rate: u32,
    #[serde(default)]
    output_wav_path: String,
    #[serde(default)]
    error: String,
}

struct StepAudioSidecarClient {
    child: Child,
    stdin: ChildStdin,
    rx: mpsc::Receiver<String>,
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
}

impl Drop for StepAudioSidecarClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Clone, Default)]
struct StepAudioCancel {
    manager: Option<Arc<TtsManager>>,
    generation: Option<u64>,
    cancel: Option<Arc<AtomicBool>>,
}

impl StepAudioCancel {
    fn for_request(manager: Arc<TtsManager>, request: &QueuedRequest) -> Self {
        Self {
            manager: Some(manager),
            generation: Some(request.generation),
            cancel: None,
        }
    }

    fn for_token(cancel: Option<Arc<AtomicBool>>) -> Self {
        Self {
            manager: None,
            generation: None,
            cancel,
        }
    }

    fn is_cancelled(&self) -> bool {
        if self
            .cancel
            .as_ref()
            .is_some_and(|cancel| cancel.load(Ordering::SeqCst))
        {
            return true;
        }
        if let (Some(manager), Some(generation)) = (&self.manager, self.generation) {
            return generation < manager.interrupt_generation.load(Ordering::SeqCst);
        }
        false
    }
}

pub(super) fn handle_step_audio_tts(
    manager: Arc<TtsManager>,
    request: QueuedRequest,
    tx: std::sync::mpsc::Sender<AudioEvent>,
) {
    let hwnd = request.req.hwnd;

    if !is_step_audio_model_downloaded() {
        if !is_step_audio_model_downloading() {
            let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
            std::thread::spawn(move || {
                let _ = download_step_audio_model(stop, true);
            });
        }
        fail_request(
            PROVIDER,
            hwnd,
            &tx,
            "Step Audio EditX model and tokenizer are downloading. Try again once the install completes.",
        );
        return;
    }

    if !is_step_audio_runtime_installed() {
        if !is_step_audio_runtime_downloading() {
            let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
            std::thread::spawn(move || {
                let _ = download_step_audio_runtime(stop, true);
            });
        }
        fail_request(
            PROVIDER,
            hwnd,
            &tx,
            "Step Audio runtime is downloading. Try again once the install completes.",
        );
        return;
    }

    match synthesize_step_audio(manager.clone(), &request) {
        Ok((samples, sample_rate)) => {
            stream_pcm_samples(&manager, &request, &tx, samples, sample_rate)
        }
        Err(err) => fail_request(PROVIDER, hwnd, &tx, format!("synthesize: {err}")),
    }
}

fn synthesize_step_audio(
    manager: Arc<TtsManager>,
    request: &QueuedRequest,
) -> Result<(Vec<i16>, u32)> {
    if let Some(issue) = step_audio_tts_text_issue(&request.req.text) {
        bail!("{issue}");
    }

    let settings = request
        .req
        .profile
        .as_ref()
        .map(|profile| profile.step_audio_settings.clone())
        .unwrap_or_else(|| {
            crate::APP
                .lock()
                .map(|app| app.config.step_audio_settings.clone())
                .unwrap_or_default()
        });
    let (prompt_audio_path, prompt_text) = resolve_reference_voice(&settings);
    if !prompt_audio_path.trim().is_empty() && prompt_text.trim().is_empty() {
        bail!(
            "Step Audio cloned voice needs the exact reference transcript. Use Auto recognize in the Reference voice library or enter it manually."
        );
    }
    let output_wav_path = super::sidecar::temp_wav_path("step-audio", request.req._id)?;
    let sidecar_request = StepAudioSidecarRequest {
        id: request.req._id.to_string(),
        operation: "clone".to_string(),
        text: request.req.text.clone(),
        voice: String::new(),
        step_model_dir: get_step_audio_editx_dir().to_string_lossy().to_string(),
        tokenizer_dir: get_step_audio_tokenizer_dir().to_string_lossy().to_string(),
        output_wav_path: output_wav_path.to_string_lossy().to_string(),
        prompt_audio_path,
        prompt_text,
        source_audio_path: String::new(),
        source_text: String::new(),
        edit_type: String::new(),
        edit_info: String::new(),
        target_text: String::new(),
    };

    run_step_audio_request(
        sidecar_request,
        output_wav_path,
        StepAudioCancel::for_request(manager, request),
    )
}

fn resolve_reference_voice(settings: &crate::config::StepAudioSettings) -> (String, String) {
    if !settings.reference_voice_id.trim().is_empty()
        && let Ok(app) = crate::APP.lock()
        && let Some(reference) = app
            .config
            .step_audio_reference_voices
            .iter()
            .find(|item| item.id == settings.reference_voice_id)
        && !reference.audio_path.trim().is_empty()
    {
        return (
            reference.audio_path.trim().to_string(),
            reference.transcript.trim().to_string(),
        );
    }

    if settings.use_custom_reference && !settings.reference_audio_path.trim().is_empty() {
        return (
            settings.reference_audio_path.trim().to_string(),
            settings.reference_text.trim().to_string(),
        );
    }

    (String::new(), String::new())
}

pub fn synthesize_step_audio_edit_to_wav(
    source_audio_path: String,
    source_text: String,
    edit_type: String,
    edit_info: String,
    target_text: String,
    cancel: Option<Arc<AtomicBool>>,
) -> Result<crate::api::tts::types::TtsCollectedAudio> {
    ensure_step_audio_ready()?;
    let request_id = crate::api::tts::manager::next_request_id_for_internal_use();
    let output_wav_path = super::sidecar::temp_wav_path("step-audio", request_id)?;
    let sidecar_request = StepAudioSidecarRequest {
        id: request_id.to_string(),
        operation: "edit".to_string(),
        text: String::new(),
        voice: String::new(),
        step_model_dir: get_step_audio_editx_dir().to_string_lossy().to_string(),
        tokenizer_dir: get_step_audio_tokenizer_dir().to_string_lossy().to_string(),
        output_wav_path: output_wav_path.to_string_lossy().to_string(),
        prompt_audio_path: String::new(),
        prompt_text: String::new(),
        source_audio_path,
        source_text,
        edit_type,
        edit_info,
        target_text,
    };
    let (samples, sample_rate) = run_step_audio_request(
        sidecar_request,
        output_wav_path,
        StepAudioCancel::for_token(cancel),
    )?;
    let duration_ms = ((samples.len() as u64) * 1000) / sample_rate.max(1) as u64;
    let wav_data = crate::api::audio::encode_wav(&samples, sample_rate, 1);
    Ok(crate::api::tts::types::TtsCollectedAudio {
        pcm_samples: samples,
        wav_data,
        sample_rate,
        duration_ms,
    })
}

fn ensure_step_audio_ready() -> Result<()> {
    if !is_step_audio_model_downloaded() {
        bail!("Step Audio EditX model and tokenizer are not installed");
    }
    if !is_step_audio_runtime_installed() {
        bail!("Step Audio runtime is not installed");
    }
    Ok(())
}

fn run_step_audio_request(
    sidecar_request: StepAudioSidecarRequest,
    output_wav_path: std::path::PathBuf,
    cancel: StepAudioCancel,
) -> Result<(Vec<i16>, u32)> {
    let response = run_sidecar(sidecar_request, cancel)?;
    if !response.ok {
        bail!(if response.error.trim().is_empty() {
            "Step Audio sidecar failed without an error message".to_string()
        } else {
            response.error
        });
    }
    let wav_path = if response.output_wav_path.trim().is_empty() {
        output_wav_path
    } else {
        std::path::PathBuf::from(response.output_wav_path)
    };
    let (samples, sample_rate) = read_wav_i16(&wav_path)?;
    let _ = std::fs::remove_file(&wav_path);
    Ok((samples, response.sample_rate.max(sample_rate)))
}

fn run_sidecar(
    request: StepAudioSidecarRequest,
    cancel: StepAudioCancel,
) -> Result<StepAudioSidecarResponse> {
    let request_id = request.id.clone();
    match run_sidecar_once(&request, &request_id, &cancel) {
        Ok(response) => Ok(response),
        Err(first_err) => {
            if cancel.is_cancelled() {
                return Err(first_err);
            }
            eprintln!("[TTS StepAudio] restarting persistent sidecar after error: {first_err}");
            let mut slot = STEP_AUDIO_SIDECAR.lock().unwrap();
            slot.take();
            drop(slot);
            run_sidecar_once(&request, &request_id, &cancel)
        }
    }
}

fn run_sidecar_once(
    request: &StepAudioSidecarRequest,
    request_id: &str,
    cancel: &StepAudioCancel,
) -> Result<StepAudioSidecarResponse> {
    let mut slot = STEP_AUDIO_SIDECAR.lock().unwrap();
    if slot.is_none() {
        *slot = Some(start_sidecar()?);
    }

    {
        let client = slot
            .as_mut()
            .ok_or_else(|| anyhow!("Step Audio sidecar did not start"))?;
        serde_json::to_writer(&mut client.stdin, request)?;
        client.stdin.write_all(b"\n")?;
        client.stdin.flush()?;
    }

    let started = Instant::now();
    let line = loop {
        if cancel.is_cancelled() {
            eprintln!("[TTS StepAudio] cancelling sidecar request {request_id}");
            slot.take();
            bail!("Generation cancelled");
        }

        let recv_result = {
            let client = slot
                .as_mut()
                .ok_or_else(|| anyhow!("Step Audio sidecar stopped"))?;
            client.rx.recv_timeout(STEP_AUDIO_CANCEL_POLL)
        };
        match recv_result {
            Ok(line) => break line,
            Err(mpsc::RecvTimeoutError::Timeout) if started.elapsed() < STEP_AUDIO_TIMEOUT => {}
            Err(err) => {
                let client = slot
                    .as_mut()
                    .ok_or_else(|| anyhow!("Step Audio sidecar stopped"))?;
                let status = match client.child.try_wait() {
                    Ok(Some(status)) => format!("exited with {status}"),
                    Ok(None) => "still running".to_string(),
                    Err(status_err) => format!("status unavailable: {status_err}"),
                };
                bail!(
                    "Step Audio sidecar timed out or stopped: {err}; process {status}{}",
                    format_step_audio_stderr_tail(&client.stderr_tail)
                );
            }
        }
    };
    let response: StepAudioSidecarResponse = serde_json::from_str(line.trim())
        .map_err(|err| anyhow!("Step Audio sidecar returned invalid JSON: {err}. stdout={line}"))?;
    super::sidecar::check_response_id("Step Audio", request_id, &response.id)?;
    Ok(response)
}

fn start_sidecar() -> Result<StepAudioSidecarClient> {
    let entrypoint = get_step_audio_runtime_entrypoint()?;
    let mut child = Command::new(&entrypoint)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| {
            format!(
                "Failed to start Step Audio sidecar '{}'",
                entrypoint.display()
            )
        })?;
    let stdin = child
        .stdin
        .take()
        .context("Step Audio sidecar stdin was unavailable")?;
    let stdout = child
        .stdout
        .take()
        .context("Step Audio sidecar stdout was unavailable")?;
    let stderr = child
        .stderr
        .take()
        .context("Step Audio sidecar stderr was unavailable")?;
    let (tx, rx) = mpsc::channel();
    let stderr_tail = Arc::new(Mutex::new(VecDeque::with_capacity(
        STEP_AUDIO_STDERR_TAIL_LINES,
    )));
    std::thread::Builder::new()
        .name("step-audio-sidecar-stdout".to_string())
        .spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(line) => {
                        if tx.send(line).is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        eprintln!("[TTS StepAudio] sidecar stdout read failed: {err}");
                        break;
                    }
                }
            }
        })
        .context("Failed to spawn Step Audio stdout reader")?;
    let stderr_tail_for_thread = stderr_tail.clone();
    std::thread::Builder::new()
        .name("step-audio-sidecar-stderr".to_string())
        .spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else {
                    break;
                };
                if !line.trim().is_empty() {
                    eprintln!("[TTS StepAudio stderr] {line}");
                    if let Ok(mut tail) = stderr_tail_for_thread.lock() {
                        if tail.len() >= STEP_AUDIO_STDERR_TAIL_LINES {
                            tail.pop_front();
                        }
                        tail.push_back(line);
                    }
                }
            }
        })
        .context("Failed to spawn Step Audio stderr reader")?;
    Ok(StepAudioSidecarClient {
        child,
        stdin,
        rx,
        stderr_tail,
    })
}

fn format_step_audio_stderr_tail(stderr_tail: &Arc<Mutex<VecDeque<String>>>) -> String {
    let Ok(tail) = stderr_tail.lock() else {
        return String::new();
    };
    if tail.is_empty() {
        return String::new();
    }
    let mut out = String::from("\nLast Step Audio stderr lines:");
    for line in tail.iter() {
        let _ = write!(out, "\n  {line}");
    }
    out
}

fn read_wav_i16(path: &std::path::Path) -> Result<(Vec<i16>, u32)> {
    let mut reader = hound::WavReader::open(path)
        .with_context(|| format!("Failed to open Step Audio WAV '{}'", path.display()))?;
    let spec = reader.spec();
    let samples = match spec.sample_format {
        hound::SampleFormat::Int => {
            if spec.bits_per_sample <= 16 {
                reader
                    .samples::<i16>()
                    .collect::<std::result::Result<Vec<_>, _>>()?
            } else {
                reader
                    .samples::<i32>()
                    .map(|sample| {
                        sample.map(|value| {
                            (value >> (spec.bits_per_sample.saturating_sub(16) as u32)) as i16
                        })
                    })
                    .collect::<std::result::Result<Vec<_>, _>>()?
            }
        }
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(|sample| {
                sample.map(|value| (value.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16)
            })
            .collect::<std::result::Result<Vec<_>, _>>()?,
    };
    Ok((samples, spec.sample_rate))
}

#[cfg(test)]
mod tests;
