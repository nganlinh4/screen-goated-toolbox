use anyhow::{Context, Result, anyhow, bail};
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::thread;
use std::time::{Duration, Instant};
use windows::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_HIGHEST,
};

use super::{AUDIO_POLL_SLEEP_MS, get_default_audio_config};

const START_TIMEOUT: Duration = Duration::from_secs(4);
const EVENT_WAIT_MS: u32 = 100;
const SILENCE_CHUNK_DIVISOR: u32 = 50;
const MAX_INITIAL_PADDING_SECONDS: u64 = 2;

pub(crate) fn record_app_audio_sidecar(
    output_path: String,
    recording_start: Instant,
    stop_signal: Arc<AtomicBool>,
    finished_signal: Arc<AtomicBool>,
    process_id: u32,
) -> Result<()> {
    let (sample_rate, channels) = get_default_audio_config();
    let channels = channels as usize;
    if sample_rate == 0 || channels == 0 {
        bail!("default system output reported an invalid audio format");
    }

    finished_signal.store(false, Ordering::SeqCst);
    let (ready_tx, ready_rx) = sync_channel(1);
    thread::spawn(move || {
        let _finished = FinishedSignal(finished_signal);
        unsafe {
            let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
        }
        if let Err(error) = capture_app_audio(
            &output_path,
            recording_start,
            &stop_signal,
            process_id,
            sample_rate,
            channels,
            ready_tx,
        ) {
            eprintln!("[DisplayAudio] per-app sidecar failed: {error:#}");
            let _ = std::fs::remove_file(output_path);
        }
    });

    match ready_rx.recv_timeout(START_TIMEOUT) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(anyhow!(error)),
        Err(error) => Err(anyhow!(
            "per-app audio sidecar did not become ready within {} ms: {error}",
            START_TIMEOUT.as_millis()
        )),
    }
}

fn capture_app_audio(
    output_path: &str,
    recording_start: Instant,
    stop_signal: &AtomicBool,
    process_id: u32,
    sample_rate: u32,
    channels: usize,
    ready_tx: SyncSender<std::result::Result<(), String>>,
) -> Result<()> {
    use wasapi::{AudioClient, Direction, SampleType, StreamMode, WaveFormat};

    let mta_result = wasapi::initialize_mta();
    if mta_result.is_err() {
        bail!("initialize audio MTA: {mta_result:?}");
    }
    let mut audio_client = AudioClient::new_application_loopback_client(process_id, true)
        .map_err(|error| anyhow!("create application loopback client: {error:?}"))?;
    let desired_format = WaveFormat::new(
        16,
        16,
        &SampleType::Int,
        sample_rate as usize,
        channels,
        None,
    );
    let mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: 1_000_000,
    };
    audio_client
        .initialize_client(&desired_format, &Direction::Capture, &mode)
        .map_err(|error| anyhow!("initialize application loopback: {error:?}"))?;
    let capture_client = audio_client
        .get_audiocaptureclient()
        .map_err(|error| anyhow!("open application capture client: {error:?}"))?;
    let event_handle = audio_client
        .set_get_eventhandle()
        .map_err(|error| anyhow!("open application capture event: {error:?}"))?;
    audio_client
        .start_stream()
        .map_err(|error| anyhow!("start application loopback: {error:?}"))?;

    let spec = hound::WavSpec {
        channels: channels as u16,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer =
        hound::WavWriter::create(output_path, spec).context("create per-app audio WAV")?;
    let mut written_frames =
        write_initial_padding(&mut writer, recording_start, sample_rate, channels)?;
    let _ = ready_tx.send(Ok(()));
    eprintln!(
        "[DisplayAudio] per-app loopback ready pid={} rate={} channels={}",
        process_id, sample_rate, channels
    );

    let mut capture_buffer = VecDeque::<u8>::new();
    let silence_chunk_frames = (sample_rate / SILENCE_CHUNK_DIVISOR).max(1) as u64;
    while !stop_signal.load(Ordering::SeqCst) {
        let _ = event_handle.wait_for_event(EVENT_WAIT_MS);
        capture_client
            .read_from_device_to_deque(&mut capture_buffer)
            .map_err(|error| anyhow!("read application loopback: {error:?}"))?;

        let captured_frames = write_captured_frames(&mut writer, &mut capture_buffer, channels)?;
        if captured_frames > 0 {
            written_frames = written_frames.saturating_add(captured_frames);
            continue;
        }

        written_frames = catch_up_with_silence(
            &mut writer,
            recording_start,
            sample_rate,
            channels,
            written_frames,
            silence_chunk_frames,
        )?;
        thread::sleep(Duration::from_millis(AUDIO_POLL_SLEEP_MS));
    }

    let _ = audio_client.stop_stream();
    let final_target = elapsed_frames(recording_start, sample_rate);
    write_silence_frames(
        &mut writer,
        final_target.saturating_sub(written_frames),
        channels,
    )?;
    writer.finalize().context("finalize per-app audio WAV")?;
    Ok(())
}

fn write_initial_padding(
    writer: &mut hound::WavWriter<std::io::BufWriter<std::fs::File>>,
    recording_start: Instant,
    sample_rate: u32,
    channels: usize,
) -> Result<u64> {
    let elapsed = recording_start
        .elapsed()
        .min(Duration::from_secs(MAX_INITIAL_PADDING_SECONDS));
    let frames = ((elapsed.as_nanos() * u128::from(sample_rate)) / 1_000_000_000) as u64;
    write_silence_frames(writer, frames, channels)?;
    Ok(frames)
}

fn write_captured_frames(
    writer: &mut hound::WavWriter<std::io::BufWriter<std::fs::File>>,
    buffer: &mut VecDeque<u8>,
    channels: usize,
) -> Result<u64> {
    let bytes_per_frame = channels.saturating_mul(2);
    if bytes_per_frame == 0 {
        return Ok(0);
    }
    let frame_count = buffer.len() / bytes_per_frame;
    for _ in 0..frame_count.saturating_mul(channels) {
        let low = buffer.pop_front().unwrap_or(0);
        let high = buffer.pop_front().unwrap_or(0);
        writer
            .write_sample(i16::from_le_bytes([low, high]))
            .context("write per-app audio sample")?;
    }
    Ok(frame_count as u64)
}

fn catch_up_with_silence(
    writer: &mut hound::WavWriter<std::io::BufWriter<std::fs::File>>,
    recording_start: Instant,
    sample_rate: u32,
    channels: usize,
    written_frames: u64,
    max_chunk_frames: u64,
) -> Result<u64> {
    let target = elapsed_frames(recording_start, sample_rate);
    let missing = target.saturating_sub(written_frames);
    let threshold = (sample_rate / SILENCE_CHUNK_DIVISOR).max(1) as u64;
    if missing < threshold {
        return Ok(written_frames);
    }
    let frames = missing.min(max_chunk_frames);
    write_silence_frames(writer, frames, channels)?;
    Ok(written_frames.saturating_add(frames))
}

fn elapsed_frames(recording_start: Instant, sample_rate: u32) -> u64 {
    ((recording_start.elapsed().as_nanos() * u128::from(sample_rate)) / 1_000_000_000) as u64
}

fn write_silence_frames(
    writer: &mut hound::WavWriter<std::io::BufWriter<std::fs::File>>,
    frames: u64,
    channels: usize,
) -> Result<()> {
    for _ in 0..frames.saturating_mul(channels as u64) {
        writer
            .write_sample(0i16)
            .context("write per-app audio silence")?;
    }
    Ok(())
}

struct FinishedSignal(Arc<AtomicBool>);

impl Drop for FinishedSignal {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}
