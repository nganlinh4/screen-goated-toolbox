//! NVIDIA Magpie-Multilingual 357M TTS worker.
//!
//! Magpie is not a libtorch DLL. It runs through a managed Python/NeMo sidecar
//! that owns PyTorch, NeMo, the Magpie checkpoint, and NanoCodec.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use super::super::manager::TtsManager;
use super::super::types::{AudioEvent, QueuedRequest};
use super::open_weights::{fail_request, stream_pcm_samples};
use crate::api::realtime_audio::magpie_assets::{
    download_magpie_model, get_magpie_checkpoint_path, get_magpie_codec_path,
    is_magpie_model_downloaded, is_magpie_model_downloading,
};
use crate::api::realtime_audio::magpie_runtime::{
    download_magpie_runtime, get_magpie_runtime_entrypoint, is_magpie_runtime_downloading,
    is_magpie_runtime_installed,
};
use crate::config::tts_catalog::{normalize_magpie_lang, resolve_magpie_voice_for_lang};

const PROVIDER: &str = "Magpie";
const MAGPIE_TIMEOUT: Duration = Duration::from_secs(180);

lazy_static::lazy_static! {
    static ref MAGPIE_SIDECAR: Mutex<Option<MagpieSidecarClient>> = Mutex::new(None);
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MagpieSidecarRequest {
    id: String,
    text: String,
    language: String,
    voice: String,
    speed: f32,
    magpie_model_path: String,
    codec_model_path: String,
    output_wav_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MagpieSidecarResponse {
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

struct MagpieSidecarClient {
    child: Child,
    stdin: ChildStdin,
    rx: mpsc::Receiver<String>,
}

impl Drop for MagpieSidecarClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub(super) fn handle_magpie_tts(
    manager: Arc<TtsManager>,
    request: QueuedRequest,
    tx: std::sync::mpsc::Sender<AudioEvent>,
) {
    let hwnd = request.req.hwnd;

    if !is_magpie_model_downloaded() {
        if !is_magpie_model_downloading() {
            let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
            std::thread::spawn(move || {
                let _ = download_magpie_model(stop, true);
            });
        }
        fail_request(
            PROVIDER,
            hwnd,
            &tx,
            "Magpie model and NanoCodec are downloading. Try again once the install completes.",
        );
        return;
    }

    if !is_magpie_runtime_installed() {
        if !is_magpie_runtime_downloading() {
            let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
            std::thread::spawn(move || {
                let _ = download_magpie_runtime(stop, true);
            });
        }
        fail_request(
            PROVIDER,
            hwnd,
            &tx,
            "Magpie runtime is downloading. Try again once the install completes.",
        );
        return;
    }

    match synthesize_magpie(&request) {
        Ok((samples, sample_rate)) => {
            stream_pcm_samples(&manager, &request, &tx, samples, sample_rate)
        }
        Err(err) => fail_request(PROVIDER, hwnd, &tx, format!("synthesize: {err}")),
    }
}

fn synthesize_magpie(request: &QueuedRequest) -> Result<(Vec<i16>, u32)> {
    let language_hint = request
        .req
        .profile
        .as_ref()
        .and_then(|profile| profile.language_code_override.as_deref())
        .map(str::to_string)
        .or_else(|| crate::lang_detect::detect_language(&request.req.text));
    let language = language_hint
        .as_deref()
        .and_then(normalize_magpie_lang)
        .ok_or_else(|| {
            anyhow!(
                "unsupported language for Magpie: '{}'. Supported: English, Spanish, German, French, Vietnamese, Italian, Mandarin Chinese, Hindi, Japanese",
                language_hint.as_deref().unwrap_or("unknown")
            )
        })?;

    let settings = request
        .req
        .profile
        .as_ref()
        .map(|profile| profile.magpie_settings.clone())
        .unwrap_or_else(|| {
            crate::APP
                .lock()
                .map(|app| app.config.magpie_settings.clone())
                .unwrap_or_default()
        });
    let voice = resolve_magpie_voice_for_lang(&settings, language_hint.as_deref());
    let output_wav_path = magpie_temp_wav_path(request.req._id)?;
    let sidecar_request = MagpieSidecarRequest {
        id: request.req._id.to_string(),
        text: request.req.text.clone(),
        language,
        voice,
        speed: 1.0,
        magpie_model_path: get_magpie_checkpoint_path().to_string_lossy().to_string(),
        codec_model_path: get_magpie_codec_path().to_string_lossy().to_string(),
        output_wav_path: output_wav_path.to_string_lossy().to_string(),
    };

    let response = run_sidecar(sidecar_request)?;
    if !response.ok {
        bail!(if response.error.trim().is_empty() {
            "Magpie sidecar failed without an error message".to_string()
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

fn run_sidecar(request: MagpieSidecarRequest) -> Result<MagpieSidecarResponse> {
    let request_id = request.id.clone();
    match run_sidecar_once(&request, &request_id) {
        Ok(response) => Ok(response),
        Err(first_err) => {
            eprintln!("[TTS Magpie] restarting persistent sidecar after error: {first_err}");
            let mut slot = MAGPIE_SIDECAR.lock().unwrap();
            slot.take();
            drop(slot);
            run_sidecar_once(&request, &request_id)
        }
    }
}

fn run_sidecar_once(
    request: &MagpieSidecarRequest,
    request_id: &str,
) -> Result<MagpieSidecarResponse> {
    let mut slot = MAGPIE_SIDECAR.lock().unwrap();
    if slot.is_none() {
        *slot = Some(start_sidecar()?);
    }
    let client = slot
        .as_mut()
        .ok_or_else(|| anyhow!("Magpie sidecar did not start"))?;
    serde_json::to_writer(&mut client.stdin, request)?;
    client.stdin.write_all(b"\n")?;
    client.stdin.flush()?;

    let line = client
        .rx
        .recv_timeout(MAGPIE_TIMEOUT)
        .map_err(|err| anyhow!("Magpie sidecar timed out or stopped: {err}"))?;
    let response: MagpieSidecarResponse = serde_json::from_str(line.trim())
        .map_err(|err| anyhow!("Magpie sidecar returned invalid JSON: {err}. stdout={line}"))?;
    if !response.id.is_empty() && response.id != request_id {
        bail!(
            "Magpie sidecar response id mismatch: expected {}, got {}",
            request_id,
            response.id
        );
    }
    Ok(response)
}

fn start_sidecar() -> Result<MagpieSidecarClient> {
    let entrypoint = get_magpie_runtime_entrypoint()?;
    let mut child = Command::new(&entrypoint)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("Failed to start Magpie sidecar '{}'", entrypoint.display()))?;
    let stdin = child
        .stdin
        .take()
        .context("Magpie sidecar stdin was unavailable")?;
    let stdout = child
        .stdout
        .take()
        .context("Magpie sidecar stdout was unavailable")?;
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("magpie-sidecar-stdout".to_string())
        .spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(line) => {
                        if tx.send(line).is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        eprintln!("[TTS Magpie] sidecar stdout read failed: {err}");
                        break;
                    }
                }
            }
        })
        .context("Failed to spawn Magpie sidecar stdout reader")?;
    eprintln!(
        "[TTS Magpie] persistent sidecar started: {}",
        entrypoint.display()
    );
    Ok(MagpieSidecarClient { child, stdin, rx })
}

fn magpie_temp_wav_path(request_id: u64) -> Result<std::path::PathBuf> {
    let dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("screen-goated-toolbox")
        .join("tmp")
        .join("magpie_tts");
    std::fs::create_dir_all(&dir)?;
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    Ok(dir.join(format!("magpie-{request_id}-{unique}.wav")))
}

fn read_wav_i16(path: &std::path::Path) -> Result<(Vec<i16>, u32)> {
    let mut reader = hound::WavReader::open(path)
        .with_context(|| format!("Failed to read Magpie WAV '{}'", path.display()))?;
    let spec = reader.spec();
    if spec.channels != 1 {
        bail!("Magpie WAV must be mono, got {} channels", spec.channels);
    }
    let samples = match spec.sample_format {
        hound::SampleFormat::Int => {
            if spec.bits_per_sample <= 16 {
                reader.samples::<i16>().collect::<Result<Vec<_>, _>>()?
            } else {
                reader
                    .samples::<i32>()
                    .map(|sample| sample.map(|value| (value >> 16) as i16))
                    .collect::<Result<Vec<_>, _>>()?
            }
        }
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(|sample| {
                sample.map(|value| (value.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16)
            })
            .collect::<Result<Vec<_>, _>>()?,
    };
    if samples.is_empty() {
        bail!("Magpie sidecar produced an empty WAV");
    }
    Ok((samples, spec.sample_rate))
}
