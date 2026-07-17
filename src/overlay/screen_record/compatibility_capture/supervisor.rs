use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::process::{
    CaptureEncoder, CaptureProcessConfig, ManagedCaptureProcess, ProcessOutcome, build_ffmpeg_args,
};

const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(20);
const RESTART_RETRY_DELAY: Duration = Duration::from_millis(120);
const PROCESS_STOP_TIMEOUT: Duration = Duration::from_secs(10);
const CONCAT_COPY_TIMEOUT: Duration = Duration::from_secs(120);
const CONCAT_REENCODE_TIMEOUT: Duration = Duration::from_secs(600);

struct ActiveSegment {
    process: ManagedCaptureProcess,
    path: PathBuf,
    started_at: Instant,
    started_offset: Duration,
}

struct CompletedSegment {
    path: PathBuf,
    started_offset: Duration,
    encoded_duration: Duration,
}

pub(super) struct CaptureSupervisor {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Vec<CompletedSegment>>>,
    ffmpeg: PathBuf,
    final_video: PathBuf,
}

impl CaptureSupervisor {
    pub(super) fn start(
        ffmpeg: PathBuf,
        config: CaptureProcessConfig,
        final_video: PathBuf,
        session_started: Instant,
    ) -> Result<Self> {
        let (first, encoder) =
            spawn_first_segment(&ffmpeg, &config, &final_video, session_started)?;
        eprintln!(
            "[DisplayCapture] encoder selected={} fps={} size={}x{}",
            encoder.label(),
            config.fps,
            config.width,
            config.height
        );
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread_ffmpeg = ffmpeg.clone();
        let thread_final_video = final_video.clone();
        let thread = std::thread::spawn(move || {
            supervise_segments(
                first,
                thread_ffmpeg,
                config,
                encoder,
                thread_final_video,
                thread_stop,
                session_started,
            )
        });

        Ok(Self {
            stop,
            thread: Some(thread),
            ffmpeg,
            final_video,
        })
    }

    pub(super) fn stop(&mut self, total_duration: Duration) -> Result<usize> {
        self.stop.store(true, Ordering::SeqCst);
        let segments = self.join_segments()?;
        let count = segments.len();
        finalize_segments(&self.ffmpeg, &segments, &self.final_video, total_duration)?;
        Ok(count)
    }

    pub(super) fn abort(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        match self.join_segments() {
            Ok(segments) => remove_segments(&segments),
            Err(error) => eprintln!("[CompatibilityCapture] supervisor abort failed: {error:#}"),
        }
    }

    fn join_segments(&mut self) -> Result<Vec<CompletedSegment>> {
        let thread = self
            .thread
            .take()
            .context("compatibility capture supervisor is not running")?;
        thread
            .join()
            .map_err(|_| anyhow::anyhow!("compatibility capture supervisor panicked"))
    }
}

impl Drop for CaptureSupervisor {
    fn drop(&mut self) {
        if self.thread.is_some() {
            self.abort();
        }
    }
}

fn supervise_segments(
    first: ActiveSegment,
    ffmpeg: PathBuf,
    config: CaptureProcessConfig,
    encoder: CaptureEncoder,
    final_video: PathBuf,
    stop: Arc<AtomicBool>,
    session_started: Instant,
) -> Vec<CompletedSegment> {
    let mut active = Some(first);
    let mut segments = Vec::new();
    let mut next_index = 1usize;

    loop {
        if stop.load(Ordering::SeqCst) {
            if let Some(mut segment) = active.take() {
                let encoded_duration = match segment.process.stop_gracefully(PROCESS_STOP_TIMEOUT) {
                    Ok(outcome) => {
                        log_process_outcome("stop", &outcome);
                        outcome.encoded_duration
                    }
                    Err(error) => {
                        eprintln!("[CompatibilityCapture] segment stop failed: {error:#}");
                        segment.started_at.elapsed()
                    }
                };
                retain_segment(segment, encoded_duration, &mut segments);
            }
            break;
        }

        if let Some(segment) = active.as_mut() {
            match segment.process.poll_exit() {
                Ok(Some(outcome)) => {
                    log_process_outcome("unexpected-exit", &outcome);
                    let ended = active.take().expect("active segment disappeared");
                    retain_segment(ended, outcome.encoded_duration, &mut segments);
                }
                Ok(None) => {
                    std::thread::sleep(PROCESS_POLL_INTERVAL);
                    continue;
                }
                Err(error) => {
                    eprintln!("[CompatibilityCapture] process polling failed: {error:#}");
                    let ended = active.take().expect("active segment disappeared");
                    let fallback_duration = ended.started_at.elapsed();
                    retain_segment(ended, fallback_duration, &mut segments);
                }
            }
        }

        while active.is_none() && !stop.load(Ordering::SeqCst) {
            match spawn_segment(
                &ffmpeg,
                &config,
                encoder,
                &final_video,
                next_index,
                session_started,
            ) {
                Ok(segment) => {
                    eprintln!(
                        "[CompatibilityCapture] capture resumed in segment {}",
                        next_index
                    );
                    active = Some(segment);
                    next_index = next_index.saturating_add(1);
                }
                Err(error) => {
                    eprintln!("[CompatibilityCapture] capture restart pending: {error:#}");
                    sleep_until_retry_or_stop(&stop);
                }
            }
        }
    }

    segments
}

fn spawn_first_segment(
    ffmpeg: &Path,
    config: &CaptureProcessConfig,
    final_video: &Path,
    session_started: Instant,
) -> Result<(ActiveSegment, CaptureEncoder)> {
    let mut failures = Vec::new();
    for encoder in CaptureEncoder::startup_order(config) {
        match spawn_segment(ffmpeg, config, encoder, final_video, 0, session_started) {
            Ok(segment) => return Ok((segment, encoder)),
            Err(error) => {
                eprintln!(
                    "[DisplayCapture] encoder {} unavailable: {error:#}",
                    encoder.label()
                );
                failures.push(format!("{}: {error:#}", encoder.label()));
            }
        }
    }
    bail!(
        "No display capture encoder could start: {}",
        failures.join(" | ")
    )
}

fn spawn_segment(
    ffmpeg: &Path,
    config: &CaptureProcessConfig,
    encoder: CaptureEncoder,
    final_video: &Path,
    index: usize,
    session_started: Instant,
) -> Result<ActiveSegment> {
    let path = segment_path(final_video, index);
    let _ = std::fs::remove_file(&path);
    let args = build_ffmpeg_args(config, encoder, &path);
    let started_at = Instant::now();
    let started_offset = started_at.saturating_duration_since(session_started);
    let mut process = ManagedCaptureProcess::spawn(ffmpeg, &args)?;
    if let Err(error) = process.verify_started() {
        drop(process);
        let _ = std::fs::remove_file(&path);
        return Err(error);
    }
    Ok(ActiveSegment {
        process,
        path,
        started_at,
        started_offset,
    })
}

fn segment_path(final_video: &Path, index: usize) -> PathBuf {
    let stem = final_video
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("compatibility-recording");
    final_video.with_file_name(format!("{stem}.part{index:03}.mp4"))
}

fn retain_segment(
    segment: ActiveSegment,
    encoded_duration: Duration,
    segments: &mut Vec<CompletedSegment>,
) {
    if is_nonempty_file(&segment.path) {
        segments.push(CompletedSegment {
            path: segment.path,
            started_offset: segment.started_offset,
            encoded_duration,
        });
    } else {
        let _ = std::fs::remove_file(segment.path);
    }
}

fn sleep_until_retry_or_stop(stop: &AtomicBool) {
    let steps = (RESTART_RETRY_DELAY.as_millis() / PROCESS_POLL_INTERVAL.as_millis()).max(1);
    for _ in 0..steps {
        if stop.load(Ordering::SeqCst) {
            return;
        }
        std::thread::sleep(PROCESS_POLL_INTERVAL);
    }
}

fn log_process_outcome(kind: &str, outcome: &ProcessOutcome) {
    eprintln!(
        "[CompatibilityCapture] segment {kind} status={} encoded_ms={} diagnostics={}",
        outcome.status,
        outcome.encoded_duration.as_millis(),
        outcome.diagnostics
    );
}

fn finalize_segments(
    ffmpeg: &Path,
    segments: &[CompletedSegment],
    final_video: &Path,
    total_duration: Duration,
) -> Result<()> {
    if segments.is_empty() {
        bail!("Compatibility capture produced no video segments");
    }
    let _ = std::fs::remove_file(final_video);

    if segments.len() == 1 {
        move_single_segment(&segments[0].path, final_video)?;
        return Ok(());
    }

    let list_path = concat_list_path(final_video);
    std::fs::write(&list_path, concat_manifest(segments, total_duration))
        .with_context(|| format!("write concat manifest {}", list_path.display()))?;

    let copy_args = concat_args(&list_path, final_video, true);
    let copy_result = run_concat(ffmpeg, &copy_args, CONCAT_COPY_TIMEOUT);
    if let Err(error) = copy_result {
        eprintln!(
            "[CompatibilityCapture] stream-copy stitch failed; retrying re-encode: {error:#}"
        );
        let _ = std::fs::remove_file(final_video);
        let encode_args = concat_args(&list_path, final_video, false);
        run_concat(ffmpeg, &encode_args, CONCAT_REENCODE_TIMEOUT)?;
    }

    let _ = std::fs::remove_file(&list_path);
    if !is_nonempty_file(final_video) {
        bail!("Compatibility capture stitching produced no video");
    }
    remove_segments(segments);
    Ok(())
}

fn move_single_segment(segment: &Path, final_video: &Path) -> Result<()> {
    if std::fs::rename(segment, final_video).is_err() {
        std::fs::copy(segment, final_video).with_context(|| {
            format!(
                "copy capture segment {} to {}",
                segment.display(),
                final_video.display()
            )
        })?;
        std::fs::remove_file(segment)
            .with_context(|| format!("remove capture segment {}", segment.display()))?;
    }
    Ok(())
}

fn concat_list_path(final_video: &Path) -> PathBuf {
    let stem = final_video
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("compatibility-recording");
    final_video.with_file_name(format!("{stem}.concat.txt"))
}

fn concat_manifest(segments: &[CompletedSegment], total_duration: Duration) -> String {
    segments
        .iter()
        .enumerate()
        .map(|(index, segment)| {
            let normalized = segment.path.to_string_lossy().replace('\\', "/");
            let next_start = segments
                .get(index + 1)
                .map(|next| next.started_offset)
                .unwrap_or(total_duration);
            let wall_slot = next_start.saturating_sub(segment.started_offset);
            let manifest_duration = wall_slot.max(segment.encoded_duration);
            let gap = manifest_duration.saturating_sub(segment.encoded_duration);
            if gap >= Duration::from_millis(20) {
                eprintln!(
                    "[CompatibilityCapture] preserving transition gap segment={} gap_ms={}",
                    index,
                    gap.as_millis()
                );
            }
            format!(
                "file '{}'\nduration {:.6}\n",
                normalized.replace('\'', "'\\''"),
                manifest_duration.as_secs_f64()
            )
        })
        .collect()
}

fn concat_args(list_path: &Path, output: &Path, stream_copy: bool) -> Vec<String> {
    let mut args = vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "warning".to_string(),
        "-y".to_string(),
        "-f".to_string(),
        "concat".to_string(),
        "-safe".to_string(),
        "0".to_string(),
        "-i".to_string(),
        list_path.to_string_lossy().to_string(),
        "-an".to_string(),
    ];
    if stream_copy {
        args.extend(["-c:v".to_string(), "copy".to_string()]);
    } else {
        args.extend([
            "-c:v".to_string(),
            "libx264".to_string(),
            "-preset".to_string(),
            "ultrafast".to_string(),
            "-tune".to_string(),
            "zerolatency".to_string(),
            "-crf".to_string(),
            "18".to_string(),
            "-pix_fmt".to_string(),
            "yuv420p".to_string(),
        ]);
    }
    args.extend([
        "-movflags".to_string(),
        "+faststart".to_string(),
        "-avoid_negative_ts".to_string(),
        "make_zero".to_string(),
        output.to_string_lossy().to_string(),
    ]);
    args
}

fn run_concat(ffmpeg: &Path, args: &[String], timeout: Duration) -> Result<()> {
    let mut process = ManagedCaptureProcess::spawn(ffmpeg, args)?;
    let outcome = process.wait_for_exit(timeout)?;
    if !outcome.status.success() {
        bail!(
            "FFmpeg segment stitch failed ({}): {}",
            outcome.status,
            outcome.diagnostics
        );
    }
    Ok(())
}

fn is_nonempty_file(path: &Path) -> bool {
    path.metadata()
        .map(|metadata| metadata.len() > 0)
        .unwrap_or(false)
}

fn remove_segments(segments: &[CompletedSegment]) {
    for segment in segments {
        let _ = std::fs::remove_file(&segment.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_paths_are_session_owned() {
        let final_path = Path::new(r"C:\recordings\recording_1_compat.mp4");
        assert_eq!(
            segment_path(final_path, 7),
            PathBuf::from(r"C:\recordings\recording_1_compat.part007.mp4")
        );
        assert_eq!(
            concat_list_path(final_path),
            PathBuf::from(r"C:\recordings\recording_1_compat.concat.txt")
        );
    }

    #[test]
    fn concat_manifest_normalizes_windows_paths() {
        let segments = [
            CompletedSegment {
                path: PathBuf::from(r"C:\recordings\one.mp4"),
                started_offset: Duration::ZERO,
                encoded_duration: Duration::from_millis(500),
            },
            CompletedSegment {
                path: PathBuf::from(r"C:\recordings\two.mp4"),
                started_offset: Duration::from_secs(1),
                encoded_duration: Duration::from_millis(500),
            },
        ];
        let manifest = concat_manifest(&segments, Duration::from_secs(2));
        assert_eq!(
            manifest,
            "file 'C:/recordings/one.mp4'\nduration 1.000000\n\
             file 'C:/recordings/two.mp4'\nduration 1.000000\n"
        );
    }
}
