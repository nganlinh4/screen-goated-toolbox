//! VieNeu-TTS v2 offline worker.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, mpsc};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use super::super::manager::TtsManager;
use super::super::types::{AudioEvent, QueuedRequest};
use super::open_weights::{fail_request, stream_pcm_samples};
use crate::api::realtime_audio::vieneu_runtime::{
    download_vieneu_runtime, get_vieneu_python_path, get_vieneu_runtime_entrypoint,
    is_vieneu_runtime_installed_for_variant,
};

const PROVIDER: &str = "VieNeu";
const TIMEOUT: Duration = Duration::from_secs(900);
const CANCEL_POLL: Duration = Duration::from_millis(200);
const STDERR_TAIL_LINES: usize = 80;
const SILENCE_WINDOW_MS: u32 = 20;
const SILENCE_PAD_MS: u32 = 140;
const MIN_AUDIBLE_PEAK: i32 = 24;
const BASE_SILENCE_PEAK_THRESHOLD: i32 = 220;

static VIENEU_SIDECAR: LazyLock<Mutex<Option<VieneuSidecarClient>>> =
    LazyLock::new(|| Mutex::new(None));

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct VieneuSidecarRequest {
    id: String,
    text: String,
    output_wav_path: String,
    mode: String,
    backbone_repo: String,
    backbone_device: String,
    codec_device: String,
    backend: String,
    emotion: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    reference_audio_path: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    reference_text: String,
    temperature: f32,
    top_k: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VieneuSidecarResponse {
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

struct VieneuSidecarClient {
    child: Child,
    stdin: ChildStdin,
    rx: mpsc::Receiver<String>,
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
}

impl Drop for VieneuSidecarClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub(super) fn handle_vieneu_tts(
    manager: Arc<TtsManager>,
    request: QueuedRequest,
    tx: std::sync::mpsc::Sender<AudioEvent>,
) {
    let hwnd = request.req.hwnd;
    let settings = vieneu_settings(&request);

    if let Err(err) = ensure_vieneu_runtime(manager.clone(), &request, &settings) {
        fail_request(PROVIDER, hwnd, &tx, err.to_string());
        return;
    }

    match synthesize_vieneu(manager.clone(), &request, settings) {
        Ok((samples, sample_rate)) => {
            stream_pcm_samples(&manager, &request, &tx, samples, sample_rate)
        }
        Err(err) => fail_request(PROVIDER, hwnd, &tx, format!("synthesize: {err}")),
    }
}

fn ensure_vieneu_runtime(
    manager: Arc<TtsManager>,
    request: &QueuedRequest,
    settings: &crate::config::VieneuSettings,
) -> Result<()> {
    if is_vieneu_runtime_installed_for_variant(&settings.variant) {
        return Ok(());
    }

    let stop = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    let generation = request.generation;
    let manager_for_cancel = manager.clone();
    let stop_for_cancel = stop.clone();
    let done_for_cancel = done.clone();
    std::thread::spawn(move || {
        while !done_for_cancel.load(Ordering::SeqCst) {
            if generation
                < manager_for_cancel
                    .interrupt_generation
                    .load(Ordering::SeqCst)
            {
                stop_for_cancel.store(true, Ordering::SeqCst);
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    });

    let result = download_vieneu_runtime(stop.clone(), true, settings.variant.clone());
    done.store(true, Ordering::SeqCst);
    result?;
    if stop.load(Ordering::SeqCst)
        || generation < manager.interrupt_generation.load(Ordering::SeqCst)
    {
        bail!("Generation cancelled");
    }
    Ok(())
}

fn vieneu_settings(request: &QueuedRequest) -> crate::config::VieneuSettings {
    request
        .req
        .profile
        .as_ref()
        .map(|profile| profile.vieneu_settings.clone())
        .unwrap_or_else(|| {
            crate::APP
                .lock()
                .map(|app| app.config.vieneu_settings.clone())
                .unwrap_or_default()
        })
}

fn synthesize_vieneu(
    manager: Arc<TtsManager>,
    request: &QueuedRequest,
    settings: crate::config::VieneuSettings,
) -> Result<(Vec<i16>, u32)> {
    let variant = crate::config::tts_catalog::vieneu_variant_by_id(&settings.variant);
    let (reference_audio_path, reference_text) = resolve_reference_voice(&settings);
    if !reference_audio_path.trim().is_empty()
        && reference_text.trim().is_empty()
        && !matches!(variant.mode, "turbo" | "turbo_gpu")
    {
        bail!("VieNeu standard/fast cloning needs the exact reference transcript.");
    }
    let output_wav_path = super::sidecar::temp_wav_path("vieneu", request.req._id)?;
    let text = normalize_vieneu_input_text(request);
    if text != request.req.text {
        eprintln!(
            "[TTS VieNeu][InputNormalize] req_id={} normalized mostly-uppercase Vietnamese input",
            request.req._id
        );
    }
    let sidecar_request = VieneuSidecarRequest {
        id: request.req._id.to_string(),
        text,
        output_wav_path: output_wav_path.to_string_lossy().to_string(),
        mode: variant.mode.to_string(),
        backbone_repo: variant.backbone_repo.to_string(),
        backbone_device: variant.backbone_device.to_string(),
        codec_device: variant.codec_device.to_string(),
        backend: variant.backend.to_string(),
        emotion: settings.emotion,
        reference_audio_path,
        reference_text,
        temperature: 0.4,
        top_k: 50,
    };
    run_request(sidecar_request, output_wav_path, manager, request)
}

fn normalize_vieneu_input_text(request: &QueuedRequest) -> String {
    let text = collapse_vieneu_input_whitespace(&request.req.text);
    if text.is_empty() {
        return String::new();
    }

    if should_lowercase_for_vieneu(&text, request) {
        text.to_lowercase()
    } else {
        text
    }
}

fn collapse_vieneu_input_whitespace(text: &str) -> String {
    text.chars()
        .map(|ch| {
            if matches!(ch, '♪' | '♫' | '♩' | '♬' | '♭' | '♮' | '♯') {
                ' '
            } else {
                ch
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn should_lowercase_for_vieneu(text: &str, request: &QueuedRequest) -> bool {
    let language_is_vietnamese = request
        .req
        .profile
        .as_ref()
        .and_then(|profile| profile.language_code_override.as_deref())
        .map(|code| matches!(code, "vie" | "vi" | "vi-VN" | "Vietnamese" | "vietnamese"))
        .unwrap_or(false);
    let has_vietnamese_mark = text.chars().any(is_vietnamese_marked_char);
    if !language_is_vietnamese && !has_vietnamese_mark {
        return false;
    }

    let mut uppercase = 0usize;
    let mut lowercase = 0usize;
    for ch in text.chars().filter(|ch| ch.is_alphabetic()) {
        if ch.is_uppercase() {
            uppercase += 1;
        } else if ch.is_lowercase() {
            lowercase += 1;
        }
    }

    uppercase >= 3 && uppercase >= lowercase.saturating_mul(2).max(1)
}

fn is_vietnamese_marked_char(ch: char) -> bool {
    const VIETNAMESE_MARKED: &str = "ÀÁÂÃÈÉÊÌÍÒÓÔÕÙÚÝàáâãèéêìíòóôõùúýĂăĐđĨĩŨũƠơƯưẠạẢảẤấẦầẨẩẪẫẬậẮắẰằẲẳẴẵẶặẸẹẺẻẼẽẾếỀềỂểỄễỆệỈỉỊịỌọỎỏỐốỒồỔổỖỗỘộỚớỜờỞởỠỡỢợỤụỦủỨứỪừỬửỮữỰựỲỳỴỵỶỷỸỹ";
    VIETNAMESE_MARKED.contains(ch)
}

fn resolve_reference_voice(settings: &crate::config::VieneuSettings) -> (String, String) {
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

fn run_request(
    sidecar_request: VieneuSidecarRequest,
    output_wav_path: std::path::PathBuf,
    manager: Arc<TtsManager>,
    request: &QueuedRequest,
) -> Result<(Vec<i16>, u32)> {
    let response = run_sidecar(sidecar_request, manager, request)?;
    if !response.ok {
        let message = if response.error.trim().is_empty() {
            "VieNeu sidecar failed without an error message".to_string()
        } else {
            response.error
        };
        bail!("{message}");
    }
    let wav_path = if response.output_wav_path.trim().is_empty() {
        output_wav_path
    } else {
        std::path::PathBuf::from(response.output_wav_path)
    };
    let (samples, sample_rate) = super::audio_utils::read_wav_i16(&wav_path, "VieNeu", false)?;
    let samples = trim_vieneu_silence(samples, sample_rate)?;
    let _ = std::fs::remove_file(&wav_path);
    Ok((samples, response.sample_rate.max(sample_rate)))
}

fn run_sidecar(
    request: VieneuSidecarRequest,
    manager: Arc<TtsManager>,
    queued: &QueuedRequest,
) -> Result<VieneuSidecarResponse> {
    let request_id = request.id.clone();
    match run_sidecar_once(&request, &request_id, &manager, queued) {
        Ok(response) => Ok(response),
        Err(first_err) => {
            if queued.generation < manager.interrupt_generation.load(Ordering::SeqCst) {
                return Err(first_err);
            }
            eprintln!("[TTS VieNeu] restarting persistent sidecar after error: {first_err}");
            let mut slot = VIENEU_SIDECAR.lock().unwrap();
            slot.take();
            drop(slot);
            run_sidecar_once(&request, &request_id, &manager, queued)
        }
    }
}

fn run_sidecar_once(
    request: &VieneuSidecarRequest,
    request_id: &str,
    manager: &TtsManager,
    queued: &QueuedRequest,
) -> Result<VieneuSidecarResponse> {
    let mut slot = VIENEU_SIDECAR.lock().unwrap();
    if slot.is_none() {
        *slot = Some(start_sidecar()?);
    }
    {
        let client = slot
            .as_mut()
            .ok_or_else(|| anyhow!("VieNeu sidecar did not start"))?;
        serde_json::to_writer(&mut client.stdin, request)?;
        client.stdin.write_all(b"\n")?;
        client.stdin.flush()?;
    }

    let started = Instant::now();
    let line = loop {
        if queued.generation < manager.interrupt_generation.load(Ordering::SeqCst) {
            eprintln!("[TTS VieNeu] cancelling sidecar request {request_id}");
            slot.take();
            bail!("Generation cancelled");
        }
        let recv_result = {
            let client = slot
                .as_mut()
                .ok_or_else(|| anyhow!("VieNeu sidecar stopped"))?;
            client.rx.recv_timeout(CANCEL_POLL)
        };
        match recv_result {
            Ok(line) => break line,
            Err(mpsc::RecvTimeoutError::Timeout) if started.elapsed() < TIMEOUT => {}
            Err(err) => {
                let client = slot
                    .as_mut()
                    .ok_or_else(|| anyhow!("VieNeu sidecar stopped"))?;
                let status = match client.child.try_wait() {
                    Ok(Some(status)) => format!("exited with {status}"),
                    Ok(None) => "still running".to_string(),
                    Err(status_err) => format!("status unavailable: {status_err}"),
                };
                bail!(
                    "VieNeu sidecar timed out or stopped: {err}; process {status}{}",
                    format_stderr_tail(&client.stderr_tail)
                );
            }
        }
    };
    let response: VieneuSidecarResponse = serde_json::from_str(line.trim())
        .map_err(|err| anyhow!("VieNeu sidecar returned invalid JSON: {err}. stdout={line}"))?;
    super::sidecar::check_response_id("VieNeu", request_id, &response.id)?;
    Ok(response)
}

fn start_sidecar() -> Result<VieneuSidecarClient> {
    let python = get_vieneu_python_path();
    let entrypoint = get_vieneu_runtime_entrypoint()?;
    let mut child = Command::new(&python)
        .arg(entrypoint)
        .env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to start VieNeu sidecar '{}'", python.display()))?;
    let stdin = child.stdin.take().context("VieNeu sidecar stdin missing")?;
    let stdout = child
        .stdout
        .take()
        .context("VieNeu sidecar stdout missing")?;
    let stderr = child
        .stderr
        .take()
        .context("VieNeu sidecar stderr missing")?;
    let (tx, rx) = mpsc::channel();
    let stderr_tail = Arc::new(Mutex::new(VecDeque::with_capacity(STDERR_TAIL_LINES)));
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(|line| line.ok()) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let stderr_tail_for_thread = stderr_tail.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(|line| line.ok()) {
            if !line.trim().is_empty() {
                eprintln!("[TTS VieNeu stderr] {line}");
                if let Ok(mut tail) = stderr_tail_for_thread.lock() {
                    if tail.len() >= STDERR_TAIL_LINES {
                        tail.pop_front();
                    }
                    tail.push_back(line);
                }
            }
        }
    });
    Ok(VieneuSidecarClient {
        child,
        stdin,
        rx,
        stderr_tail,
    })
}

fn format_stderr_tail(stderr_tail: &Arc<Mutex<VecDeque<String>>>) -> String {
    let Ok(tail) = stderr_tail.lock() else {
        return String::new();
    };
    if tail.is_empty() {
        return String::new();
    }
    let mut out = String::from("\nLast VieNeu stderr lines:");
    for line in tail.iter() {
        out.push_str("\n  ");
        out.push_str(line);
    }
    out
}


fn trim_vieneu_silence(samples: Vec<i16>, sample_rate: u32) -> Result<Vec<i16>> {
    if samples.is_empty() || sample_rate == 0 {
        bail!("VieNeu returned empty audio");
    }

    let window = ((sample_rate as usize * SILENCE_WINDOW_MS as usize) / 1000).max(1);
    let pad = ((sample_rate as usize * SILENCE_PAD_MS as usize) / 1000).max(1);
    let peak = samples
        .iter()
        .map(|sample| (*sample as i32).abs())
        .max()
        .unwrap_or(0);
    if peak < MIN_AUDIBLE_PEAK {
        bail!("VieNeu returned only silence");
    }
    let adaptive = ((peak as f32) * 0.03).round() as i32;
    let threshold = BASE_SILENCE_PEAK_THRESHOLD.min(adaptive.max(MIN_AUDIBLE_PEAK));

    let mut first = None;
    let mut last = None;
    for (index, chunk) in samples.chunks(window).enumerate() {
        let peak = chunk
            .iter()
            .map(|sample| (*sample as i32).abs())
            .max()
            .unwrap_or(0);
        if peak >= threshold {
            let start = index * window;
            first.get_or_insert(start);
            last = Some((start + chunk.len()).min(samples.len()));
        }
    }

    let Some(first_signal) = first else {
        eprintln!("[TTS VieNeu] silence trim skipped: peak={peak}");
        return Ok(samples);
    };
    let last_signal = last.unwrap_or(first_signal);
    let start = first_signal.saturating_sub(pad);
    let end = (last_signal + pad).min(samples.len());
    if end <= start {
        eprintln!("[TTS VieNeu] silence trim skipped: invalid range peak={peak}");
        return Ok(samples);
    }

    let trimmed = samples[start..end].to_vec();
    if trimmed.len() != samples.len() {
        eprintln!(
            "[TTS VieNeu] trimmed silence: {}ms -> {}ms",
            samples.len() as u64 * 1000 / sample_rate as u64,
            trimmed.len() as u64 * 1000 / sample_rate as u64
        );
    }
    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tts::types::TtsRequest;

    fn queued(text: &str) -> QueuedRequest {
        QueuedRequest {
            req: TtsRequest {
                _id: 1,
                text: text.to_string(),
                hwnd: 0,
                is_realtime: false,
                profile: None,
            },
            generation: 0,
        }
    }

    #[test]
    fn vieneu_lowercases_all_caps_vietnamese() {
        let request = queued("TÔI VÀ CÔ GÁI CỦA TÔI, CHÚNG TÔI CÓ\nMỐI QUAN HỆ NÀY");
        assert_eq!(
            normalize_vieneu_input_text(&request),
            "tôi và cô gái của tôi, chúng tôi có mối quan hệ này"
        );
    }

    #[test]
    fn vieneu_preserves_mixed_case_vietnamese() {
        let request = queued("Tôi và cô gái của tôi");
        assert_eq!(
            normalize_vieneu_input_text(&request),
            "Tôi và cô gái của tôi"
        );
    }

    #[test]
    fn vieneu_collapses_lyric_line_breaks() {
        let request = queued("♪ TÔI, TÔI VÀ LOUIE, CHÚNG TÔI\nSẼ CHẠY ĐẾN BÊN ♪");
        assert_eq!(
            normalize_vieneu_input_text(&request),
            "tôi, tôi và louie, chúng tôi sẽ chạy đến bên"
        );
    }
}
