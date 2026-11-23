use anyhow::{Context, Result, bail};
use cpal::traits::{DeviceTrait, HostTrait};
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::thread;
use std::time::{Duration, Instant};
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_HIGHEST,
};

#[cfg(test)]
mod live_tests;
mod timeline;
use timeline::{AudioPacket, Timeline};

// Normal 10ms packets tolerate over a second of writer scheduling delay.
// The queue's maximum PCM allocation remains bounded to 32 MiB.
const PACKET_QUEUE_CAPACITY: usize = 128;
const MAX_PACKET_SAMPLES: usize = 128 * 1024;

#[derive(Default)]
pub(crate) struct DeviceSidecarState {
    stop_at: Mutex<Option<Duration>>,
    error: Mutex<Option<String>>,
}

impl DeviceSidecarState {
    pub(crate) fn stop_now(&self, recording_start: Instant) -> Duration {
        self.stop_with(|| recording_start.elapsed())
    }

    fn stop_with(&self, now: impl FnOnce() -> Duration) -> Duration {
        let mut stop_at = self.stop_at.lock();
        let elapsed = now();
        *stop_at = Some(elapsed);
        elapsed
    }

    fn packet_limit(
        &self,
        packet_end: u64,
        sample_rate: u32,
        now: impl FnOnce() -> Duration,
    ) -> Option<Option<u64>> {
        let stop = self.stop_at.lock();
        if let Some(stop_at) = *stop {
            return Some(Some(timeline::duration_frames(stop_at, sample_rate)));
        }
        (timeline::duration_frames(now(), sample_rate) >= packet_end).then_some(None)
    }

    pub(crate) fn error(&self) -> Option<String> {
        self.error.lock().clone()
    }
}

struct CaptureClock {
    recording_start: Instant,
    sample_rate: u32,
    channels: usize,
    qpc_offset_ns: i128,
    previous_packet: Option<(i128, i128)>,
}

impl CaptureClock {
    fn calibrate(recording_start: Instant, sample_rate: u32, channels: usize) -> Result<Self> {
        let mut frequency = 0i64;
        unsafe { QueryPerformanceFrequency(&mut frequency) }
            .context("query capture clock frequency")?;
        if frequency <= 0 {
            bail!("invalid capture clock frequency");
        }
        let before = Instant::now();
        let mut counter = 0i64;
        unsafe { QueryPerformanceCounter(&mut counter) }.context("query capture clock")?;
        let after = Instant::now();
        if counter < 0 || after.duration_since(before) > Duration::from_millis(5) {
            bail!("capture clock calibration was interrupted");
        }
        let midpoint = before + after.duration_since(before) / 2;
        let elapsed = midpoint.saturating_duration_since(recording_start);
        let qpc_ns = (i128::from(counter) * 1_000_000_000) / i128::from(frequency);
        Ok(Self {
            recording_start,
            sample_rate,
            channels,
            qpc_offset_ns: elapsed.as_nanos() as i128 - qpc_ns,
            previous_packet: None,
        })
    }

    fn capture_start_ns(&self, timestamp: cpal::InputStreamTimestamp) -> Option<i128> {
        self.qpc_offset_ns
            .checked_add(i128::try_from(timestamp.capture.as_nanos()).ok()?)
    }
}

fn submit_packet<T>(
    sender: &SyncSender<AudioPacket>,
    failed: &AtomicBool,
    clock: &mut CaptureClock,
    info: &cpal::InputCallbackInfo,
    data: &[T],
    convert: impl Fn(&T) -> i16,
) {
    if data.is_empty() || failed.load(Ordering::Relaxed) {
        return;
    }
    if data.len() > MAX_PACKET_SAMPLES || !data.len().is_multiple_of(clock.channels) {
        crate::log_info!("[CompatibilityAudio] rejected_packet reason=invalid_sample_count");
        failed.store(true, Ordering::Release);
        return;
    }
    let timestamp = info.timestamp();
    let observed_elapsed = clock.recording_start.elapsed();
    let Some(mut capture_ns) = clock.capture_start_ns(timestamp) else {
        crate::log_info!("[CompatibilityAudio] rejected_packet reason=timestamp_overflow");
        failed.store(true, Ordering::Release);
        return;
    };
    // A loopback stream created after recording starts may return a small
    // amount of buffered pre-roll, but an ancient QPC value is invalid.
    if timestamp.capture.as_nanos() == 0 || capture_ns < -10_000_000_000 {
        crate::log_info!(
            "[CompatibilityAudio] rejected_packet reason=invalid_epoch capture_ns={capture_ns}"
        );
        failed.store(true, Ordering::Release);
        return;
    }
    let packet_frames = data.len() / clock.channels;
    let packet_duration_ns =
        (packet_frames as i128 * 1_000_000_000) / i128::from(clock.sample_rate);
    let ahead_ns = capture_ns
        .saturating_add(packet_duration_ns)
        .saturating_sub(observed_elapsed.as_nanos() as i128);
    // Some endpoint clocks report a fixed presentation-time offset. A bounded
    // future first packet establishes that offset once; scheduling delays and
    // later silence never re-anchor the sample clock. The separate callback
    // clock may be stale or zero and cannot establish packet validity.
    if clock.previous_packet.is_none() && (5_000_001..=1_000_000_000).contains(&ahead_ns) {
        clock.qpc_offset_ns -= ahead_ns;
        capture_ns -= ahead_ns;
    }
    if clock
        .previous_packet
        .is_some_and(|(previous_start, previous_end)| {
            capture_ns <= previous_start || capture_ns.saturating_add(5_000_000) < previous_end
        })
    {
        crate::log_info!(
            "[CompatibilityAudio] rejected_packet reason=timestamp_regression capture_ns={capture_ns} previous={:?}",
            clock.previous_packet
        );
        failed.store(true, Ordering::Release);
        return;
    }
    clock.previous_packet = Some((capture_ns, capture_ns.saturating_add(packet_duration_ns)));
    // Callback scheduling latency never changes the established mapping.
    let rate = i128::from(clock.sample_rate);
    let start_frame =
        u64::try_from(capture_ns.max(0).saturating_mul(rate) / 1_000_000_000).unwrap_or(u64::MAX);
    let skip_frames = if capture_ns < 0 {
        usize::try_from(
            capture_ns
                .saturating_neg()
                .saturating_mul(rate)
                .saturating_add(999_999_999)
                / 1_000_000_000,
        )
        .unwrap_or(usize::MAX)
    } else {
        0
    }
    .min(data.len() / clock.channels);
    let available_frames = timeline::duration_frames(observed_elapsed, clock.sample_rate)
        .saturating_sub(start_frame) as usize;
    let remaining_frames = data.len() / clock.channels - skip_frames;
    let tolerance = (clock.sample_rate as usize / 200).max(1);
    if remaining_frames > available_frames.saturating_add(tolerance) {
        crate::log_info!(
            "[CompatibilityAudio] rejected_packet reason=future_packet remaining={remaining_frames} available={available_frames} start_frame={start_frame}"
        );
        failed.store(true, Ordering::Release);
        return;
    }
    let take_frames = remaining_frames;
    if take_frames == 0 {
        // A bounded pre-recording packet is harmless.
        return;
    }
    let start = skip_frames * clock.channels;
    let end = start + take_frames * clock.channels;
    let packet = AudioPacket {
        start_frame,
        samples: data[start..end].iter().map(convert).collect(),
    };
    queue_packet(sender, failed, packet);
}

fn queue_packet(sender: &SyncSender<AudioPacket>, failed: &AtomicBool, packet: AudioPacket) {
    if let Err(error) = sender.try_send(packet) {
        let reason = match error {
            std::sync::mpsc::TrySendError::Full(_) => "queue_full",
            std::sync::mpsc::TrySendError::Disconnected(_) => "writer_disconnected",
        };
        crate::log_info!("[CompatibilityAudio] rejected_packet reason={reason}");
        // Dropping a packet would silently shorten the sidecar. Fail the audio
        // output instead of returning a misleading, desynchronized recording.
        failed.store(true, Ordering::Release);
    }
}

fn write_captured_packet<W: std::io::Write + std::io::Seek>(
    writer: &mut hound::WavWriter<W>,
    timeline: &mut Timeline,
    packet: AudioPacket,
    state: &DeviceSidecarState,
    recording_start: Instant,
    sample_rate: u32,
    channels: usize,
) -> Result<()> {
    let packet_end = packet
        .start_frame
        .saturating_add((packet.samples.len() / channels) as u64);
    loop {
        // The stop timestamp and the decision to write a complete packet are
        // sampled under one lock. No disk I/O occurs while holding it.
        if let Some(limit) =
            state.packet_limit(packet_end, sample_rate, || recording_start.elapsed())
        {
            return timeline.write_packet_until(writer, packet, limit);
        }
        // Only the 5ms clock-calibration tolerance may require this wait.
        thread::sleep(Duration::from_millis(1));
    }
}

fn build_loopback_stream(
    config: &cpal::StreamConfig,
    recording_start: Instant,
    sender: SyncSender<AudioPacket>,
    failed: Arc<AtomicBool>,
) -> Result<super::follow_default::DefaultLoopback> {
    let mut clock = CaptureClock::calibrate(
        recording_start,
        config.sample_rate,
        usize::from(config.channels),
    )?;
    let stream_failed = failed.clone();
    let err_fn = move |error| {
        handle_stream_error(&stream_failed, error);
    };
    let mut route = 0;
    super::follow_default::DefaultLoopback::start(
        config.sample_rate,
        usize::from(config.channels),
        move |data: &[f32], info: &_, generation| {
            if route != generation {
                match CaptureClock::calibrate(recording_start, clock.sample_rate, clock.channels) {
                    Ok(recalibrated) => clock = recalibrated,
                    Err(error) => {
                        eprintln!(
                            "[CompatibilityAudio] endpoint clock calibration failed: {error}"
                        );
                        failed.store(true, Ordering::Release);
                        return;
                    }
                }
                route = generation;
            }
            submit_packet(&sender, &failed, &mut clock, info, data, |sample| {
                (sample.clamp(-1.0, 1.0) * 32767.0) as i16
            })
        },
        err_fn,
    )
    .context("build system-output loopback stream")
}

pub(crate) fn record_device_audio_sidecar(
    output_path: String,
    recording_start: Instant,
    stop_signal: Arc<AtomicBool>,
    finished_signal: Arc<AtomicBool>,
    state: Arc<DeviceSidecarState>,
) -> Result<()> {
    finished_signal.store(false, Ordering::SeqCst);
    let host = cpal::host_from_id(cpal::HostId::Wasapi)
        .context("initialize WASAPI system-output capture")?;
    let device = host
        .default_output_device()
        .context("no default system output device found")?;
    let config = device
        .default_output_config()
        .context("query default system output format")?;
    let sample_rate = config.sample_rate();
    let channels = usize::from(config.channels());
    if sample_rate == 0 || channels == 0 {
        bail!("default system output reported an invalid audio format");
    }
    let (sender, receiver) = sync_channel(PACKET_QUEUE_CAPACITY);
    let failed = Arc::new(AtomicBool::new(false));
    let stream = build_loopback_stream(&config.into(), recording_start, sender, failed.clone())?;
    thread::spawn(move || {
        unsafe {
            let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
        }
        let spec = hound::WavSpec {
            channels: channels as u16,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let result = (|| -> Result<()> {
            let mut writer =
                hound::WavWriter::create(&output_path, spec).context("create system-output WAV")?;
            let mut timeline = Timeline::new(sample_rate, channels);
            while !stop_signal.load(Ordering::SeqCst) && !failed.load(Ordering::Acquire) {
                match receiver.recv_timeout(Duration::from_millis(10)) {
                    Ok(packet) => {
                        write_captured_packet(
                            &mut writer,
                            &mut timeline,
                            packet,
                            &state,
                            recording_start,
                            sample_rate,
                            channels,
                        )?;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        bail!("system-output stream stopped unexpectedly")
                    }
                }
            }
            let stop_at = state
                .stop_at
                .lock()
                .unwrap_or_else(|| recording_start.elapsed());
            let final_frame = timeline::duration_frames(stop_at, sample_rate);
            // Stop callbacks before draining the queue and writing the final tail.
            drop(stream);
            while let Ok(packet) = receiver.try_recv() {
                timeline.write_packet_until(&mut writer, packet, Some(final_frame))?;
            }
            if failed.load(Ordering::Acquire) {
                bail!("system-output audio packet was lost or invalid");
            }
            timeline.finish(&mut writer, final_frame)?;
            writer.finalize().context("finalize system-output WAV")?;
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("[CompatibilityAudio] capture failed: {error:#}");
            *state.error.lock() = Some(format!("System audio capture failed: {error:#}"));
        }
        finished_signal.store(true, Ordering::SeqCst);
    });
    Ok(())
}

fn handle_stream_error(failed: &AtomicBool, error: cpal::Error) {
    eprintln!("[CompatibilityAudio] stream event: {error}");
    if !matches!(
        error.kind(),
        cpal::ErrorKind::Xrun | cpal::ErrorKind::DeviceChanged | cpal::ErrorKind::RealtimeDenied
    ) {
        failed.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ancient_qpc_packet_fails_instead_of_producing_silence() {
        let mut clock = CaptureClock {
            recording_start: Instant::now(),
            sample_rate: 48_000,
            channels: 1,
            qpc_offset_ns: -100_000_000_000,
            previous_packet: None,
        };
        for timestamp in [
            cpal::StreamInstant::new(0, 0),
            cpal::StreamInstant::new(1, 0),
        ] {
            let info = cpal::InputCallbackInfo::new(cpal::InputStreamTimestamp {
                capture: timestamp,
                callback: timestamp,
            });
            let (sender, receiver) = sync_channel(1);
            let failed = AtomicBool::new(false);
            submit_packet(
                &sender,
                &failed,
                &mut clock,
                &info,
                &[1i16; 480],
                |sample| *sample,
            );
            assert!(failed.load(Ordering::Acquire));
            assert!(receiver.try_recv().is_err());
        }
    }

    #[test]
    fn frozen_preroll_timestamp_fails_before_silence_can_be_reported() {
        let mut clock = CaptureClock {
            recording_start: Instant::now(),
            sample_rate: 48_000,
            channels: 1,
            qpc_offset_ns: -2_000_000_000,
            previous_packet: None,
        };
        let info = cpal::InputCallbackInfo::new(cpal::InputStreamTimestamp {
            capture: cpal::StreamInstant::new(1, 0),
            callback: cpal::StreamInstant::new(1, 10_000_000),
        });
        let (sender, receiver) = sync_channel(2);
        let failed = AtomicBool::new(false);
        submit_packet(
            &sender,
            &failed,
            &mut clock,
            &info,
            &[1i16; 480],
            |sample| *sample,
        );
        assert!(!failed.load(Ordering::Acquire));
        submit_packet(
            &sender,
            &failed,
            &mut clock,
            &info,
            &[1i16; 480],
            |sample| *sample,
        );
        assert!(failed.load(Ordering::Acquire));
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn delayed_first_callback_and_prompt_backlog_use_calibrated_qpc_time() {
        let mut clock = CaptureClock {
            recording_start: Instant::now() - Duration::from_millis(2060),
            sample_rate: 48_000,
            channels: 2,
            qpc_offset_ns: -8_000_000_000,
            previous_packet: None,
        };
        let timestamp = |capture_ms: u32, callback_ms: u32| {
            cpal::InputCallbackInfo::new(cpal::InputStreamTimestamp {
                capture: cpal::StreamInstant::new(10, capture_ms * 1_000_000),
                callback: cpal::StreamInstant::new(10, callback_ms * 1_000_000),
            })
        };
        let (sender, receiver) = sync_channel(2);
        let failed = AtomicBool::new(false);
        let samples = vec![1i16; 960];
        submit_packet(
            &sender,
            &failed,
            &mut clock,
            &timestamp(0, 10),
            &samples,
            |sample| *sample,
        );
        submit_packet(
            &sender,
            &failed,
            &mut clock,
            &timestamp(10, 20),
            &samples,
            |sample| *sample,
        );
        assert!(!failed.load(Ordering::Acquire));
        assert_eq!(receiver.try_recv().unwrap().start_frame, 96_000);
        assert_eq!(receiver.try_recv().unwrap().start_frame, 96_480);
    }

    #[test]
    fn packet_admission_and_stop_snapshot_are_ordered_without_holding_disk_io() {
        let state = Arc::new(DeviceSidecarState::default());
        let (in_admission, entered) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let writer_state = state.clone();
        let writer = thread::spawn(move || {
            writer_state.packet_limit(1, 100, || {
                in_admission.send(()).unwrap();
                released.recv().unwrap();
                Duration::from_millis(10)
            })
        });
        entered.recv().unwrap();
        let stop_state = state.clone();
        let stop = thread::spawn(move || stop_state.stop_with(|| Duration::from_millis(20)));
        release.send(()).unwrap();
        assert_eq!(writer.join().unwrap(), Some(None));
        assert_eq!(stop.join().unwrap(), Duration::from_millis(20));
        assert_eq!(
            state.packet_limit(3, 100, || panic!("stopped packet must be clipped")),
            Some(Some(2))
        );
    }

    #[test]
    fn overflow_and_disconnected_writer_are_reported_as_packet_failure() {
        let (sender, receiver) = sync_channel(1);
        let failed = AtomicBool::new(false);
        let packet = || AudioPacket {
            start_frame: 0,
            samples: vec![1],
        };
        queue_packet(&sender, &failed, packet());
        assert!(!failed.load(Ordering::Acquire));
        queue_packet(&sender, &failed, packet());
        assert!(failed.load(Ordering::Acquire));
        failed.store(false, Ordering::Release);
        drop(receiver);
        queue_packet(&sender, &failed, packet());
        assert!(failed.load(Ordering::Acquire));
    }

    #[test]
    fn packet_queue_tolerates_a_quarter_second_writer_scheduling_pause() {
        let (sender, receiver) = sync_channel(PACKET_QUEUE_CAPACITY);
        let failed = AtomicBool::new(false);
        for index in 0..25 {
            queue_packet(
                &sender,
                &failed,
                AudioPacket {
                    start_frame: index * 480,
                    samples: vec![0; 960],
                },
            );
        }
        assert!(!failed.load(Ordering::Acquire));
        assert_eq!(receiver.try_iter().count(), 25);
        assert!(
            PACKET_QUEUE_CAPACITY * MAX_PACKET_SAMPLES * std::mem::size_of::<i16>()
                <= 32 * 1024 * 1024
        );
    }
}
