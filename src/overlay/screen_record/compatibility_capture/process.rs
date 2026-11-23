use crate::process_job::{HelperCommand, Input, LaunchSlot, OwnedProcess};
use anyhow::{Context, Result, bail};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
const STARTUP_GRACE: Duration = Duration::from_millis(400);
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(10);
const STDERR_TAIL_LINES: usize = 32;
static LAUNCH: LaunchSlot = LaunchSlot::new();
#[cfg(test)]
pub(super) static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Clone, Debug)]
pub(super) struct CaptureProcessConfig {
    pub(super) monitor_index: usize,
    pub(super) fps: u32,
    pub(super) include_cursor: bool,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) bitrate: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CaptureEncoder {
    MediaFoundation,
    Software,
}

impl CaptureEncoder {
    pub(super) fn startup_order(config: &CaptureProcessConfig) -> Vec<Self> {
        if config.width.is_multiple_of(2) && config.height.is_multiple_of(2) {
            vec![Self::MediaFoundation, Self::Software]
        } else {
            vec![Self::Software]
        }
    }

    /// The next encoder to try once this one keeps failing mid-recording.
    pub(super) fn fallback(self, config: &CaptureProcessConfig) -> Option<Self> {
        let order = Self::startup_order(config);
        let index = order.iter().position(|encoder| *encoder == self)?;
        order.get(index + 1).copied()
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::MediaFoundation => "h264_mf(hardware)",
            Self::Software => "libx264(software)",
        }
    }
}

pub(super) struct ProcessOutcome {
    pub(super) status: ExitStatus,
    pub(super) diagnostics: String,
    pub(super) encoded_duration: Duration,
}

pub(super) struct ManagedCaptureProcess {
    child: OwnedProcess,
    stdin: Option<Input>,
    stderr_thread: Option<JoinHandle<()>>,
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
    progress_us: Arc<AtomicU64>,
    first_frame: Arc<Mutex<Option<Instant>>>,
}

#[derive(Clone)]
pub(super) struct ResolvedFfmpeg {
    path: PathBuf,
    #[cfg(not(feature = "recorder-worker"))]
    _component: Arc<crate::component_registry::external_tools::ExternalToolUse>,
}

impl std::ops::Deref for ResolvedFfmpeg {
    type Target = Path;
    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

#[cfg(all(test, feature = "recorder-worker"))]
pub(super) fn test_executable() -> ResolvedFfmpeg {
    ResolvedFfmpeg {
        path: std::env::current_exe().unwrap(),
    }
}

impl ManagedCaptureProcess {
    pub(super) fn spawn(ffmpeg: &ResolvedFfmpeg, args: &[String]) -> Result<Self> {
        let frame_clock = super::frame_clock::FrameClock::now()?;
        let permit = LAUNCH.acquire()?;
        let mut command = HelperCommand::new(&ffmpeg.path);
        command.args(args).pipe_stdin().stdout_null();

        let mut child = OwnedProcess::spawn_helper(&command, (ffmpeg.clone(), permit))
            .with_context(|| format!("start FFmpeg at {}", ffmpeg.display()))?;
        let stdin = child.take_stdin();
        let stderr = child.take_stderr().context("open FFmpeg stderr")?;
        child.resume()?;
        let stderr_tail = Arc::new(Mutex::new(VecDeque::new()));
        let reader_tail = stderr_tail.clone();
        let progress_us = Arc::new(AtomicU64::new(0));
        let reader_progress_us = progress_us.clone();
        let first_frame = Arc::new(Mutex::new(None));
        let reader_first_frame = first_frame.clone();
        let stderr_thread = std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(std::result::Result::ok) {
                if line.contains("Parsed_showinfo_") {
                    if let Some(origin) = frame_clock.first_frame(&line) {
                        reader_first_frame.lock().get_or_insert(origin);
                    }
                    continue;
                }
                if let Some(value) = line.strip_prefix("out_time_us=") {
                    if let Ok(value) = value.parse::<u64>() {
                        reader_progress_us.store(value, Ordering::Relaxed);
                    }
                    continue;
                }
                if is_progress_line(&line) {
                    continue;
                }
                eprintln!("[CompatibilityCapture][FFmpeg] {line}");
                let mut tail = reader_tail.lock();
                if tail.len() == STDERR_TAIL_LINES {
                    tail.pop_front();
                }
                tail.push_back(line);
            }
        });

        Ok(Self {
            child,
            stdin,
            stderr_thread: Some(stderr_thread),
            stderr_tail,
            progress_us,
            first_frame,
        })
    }

    pub(super) fn verify_started(&mut self) -> Result<()> {
        let began = Instant::now();
        loop {
            if let Some(outcome) = self.poll_exit()? {
                bail!(
                    "FFmpeg compatibility capture exited during startup ({}): {}",
                    outcome.status,
                    outcome.diagnostics
                );
            }
            if began.elapsed() >= STARTUP_GRACE && self.first_frame.lock().is_some() {
                return Ok(());
            }
            if began.elapsed() >= FIRST_FRAME_TIMEOUT {
                bail!(
                    "FFmpeg did not report its first captured frame: {}",
                    self.stderr_summary()
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub(super) fn started_at(&self) -> Option<Instant> {
        *self.first_frame.lock()
    }

    pub(super) fn poll_exit(&mut self) -> Result<Option<ProcessOutcome>> {
        let Some(status) = self.child.try_wait().context("poll FFmpeg exit")? else {
            return Ok(None);
        };
        self.child.stop_within(Duration::from_secs(3))?;
        self.join_stderr()?;
        Ok(Some(ProcessOutcome {
            status,
            diagnostics: self.stderr_summary(),
            encoded_duration: Duration::from_micros(self.progress_us.load(Ordering::Relaxed)),
        }))
    }

    pub(super) fn stop_gracefully(&mut self, timeout: Duration) -> Result<ProcessOutcome> {
        let mut stop_signal_error = None;
        if let Some(mut stdin) = self.stdin.take()
            && let Err(error) = stdin.write_all(b"q\n").and_then(|()| stdin.flush())
        {
            stop_signal_error = Some(error.to_string());
        }

        let deadline = Instant::now() + timeout;
        loop {
            if let Some(mut outcome) = self.poll_exit()? {
                if let Some(error) = stop_signal_error.as_deref() {
                    outcome.diagnostics = format!(
                        "{}{}",
                        stop_signal_context(Some(error)),
                        outcome.diagnostics
                    );
                }
                return Ok(outcome);
            }
            if Instant::now() >= deadline {
                self.child.stop_within(Duration::from_secs(3))?;
                let status = self
                    .child
                    .try_wait()?
                    .context("FFmpeg exit was not reaped")?;
                self.join_stderr()?;
                bail!(
                    "FFmpeg compatibility capture did not stop within {} ms ({status}){}: {}",
                    timeout.as_millis(),
                    stop_signal_context(stop_signal_error.as_deref()),
                    self.stderr_summary()
                );
            }
            std::thread::sleep(Duration::from_millis(40));
        }
    }

    pub(super) fn wait_for_exit(&mut self, timeout: Duration) -> Result<ProcessOutcome> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(outcome) = self.poll_exit()? {
                return Ok(outcome);
            }
            if Instant::now() >= deadline {
                self.child.stop_within(Duration::from_secs(3))?;
                let status = self
                    .child
                    .try_wait()?
                    .context("FFmpeg exit was not reaped")?;
                self.join_stderr()?;
                bail!(
                    "FFmpeg process exceeded {} ms ({status}): {}",
                    timeout.as_millis(),
                    self.stderr_summary()
                );
            }
            std::thread::sleep(Duration::from_millis(40));
        }
    }

    fn join_stderr(&mut self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(2);
        while self
            .stderr_thread
            .as_ref()
            .is_some_and(|thread| !thread.is_finished())
        {
            if Instant::now() >= deadline {
                bail!("FFmpeg diagnostic output has not drained");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        if let Some(thread) = self.stderr_thread.take() {
            thread
                .join()
                .map_err(|_| anyhow::anyhow!("FFmpeg diagnostic reader failed"))?;
        }
        Ok(())
    }

    fn stderr_summary(&self) -> String {
        let tail = self.stderr_tail.lock();
        if tail.is_empty() {
            "no FFmpeg diagnostics".to_string()
        } else {
            tail.iter().cloned().collect::<Vec<_>>().join(" | ")
        }
    }
}

impl Drop for ManagedCaptureProcess {
    fn drop(&mut self) {
        self.stdin.take();
        let _ = self.child.stop_within(Duration::from_millis(500));
        if self.child.stopped() {
            let _ = self.join_stderr();
        }
    }
}

#[cfg(feature = "recorder-worker")]
pub(super) fn resolve_ffmpeg() -> Result<ResolvedFfmpeg> {
    crate::gui::settings_ui::download_manager::ffmpeg_dependency::ensure_ffmpeg_with_badge()
        .map(|path| ResolvedFfmpeg { path })
        .map_err(anyhow::Error::new)
}

#[cfg(not(feature = "recorder-worker"))]
pub(super) fn resolve_ffmpeg() -> Result<ResolvedFfmpeg> {
    let component =
        crate::gui::settings_ui::download_manager::ffmpeg_dependency::acquire_ffmpeg_with_badge()
            .map_err(anyhow::Error::msg)?;
    Ok(ResolvedFfmpeg {
        path: component.executable(),
        _component: Arc::new(component),
    })
}

pub(super) fn build_ffmpeg_args(
    config: &CaptureProcessConfig,
    encoder: CaptureEncoder,
    output: &Path,
) -> Vec<String> {
    let mut args = vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "info".to_string(),
        "-nostats".to_string(),
        "-progress".to_string(),
        "pipe:2".to_string(),
        "-stats_period".to_string(),
        "0.25".to_string(),
        "-y".to_string(),
        "-copyts".to_string(),
        "-use_wallclock_as_timestamps".to_string(),
        "1".to_string(),
        "-f".to_string(),
        "lavfi".to_string(),
        "-i".to_string(),
        format!(
            "ddagrab=output_idx={}:framerate={}:draw_mouse={}",
            config.monitor_index,
            config.fps,
            if config.include_cursor { 1 } else { 0 }
        ),
        "-an".to_string(),
    ];
    let clock_filter = "settb=1/1000000,showinfo=checksum=0,setpts=PTS-STARTPTS";

    match encoder {
        CaptureEncoder::MediaFoundation => {
            let max_bitrate = u64::from(config.bitrate)
                .saturating_mul(5)
                .div_ceil(4)
                .min(100_000_000);
            args.extend([
                "-vf".to_string(),
                clock_filter.to_string(),
                "-c:v".to_string(),
                "h264_mf".to_string(),
                "-hw_encoding".to_string(),
                "1".to_string(),
                "-rate_control".to_string(),
                "pc_vbr".to_string(),
                "-scenario".to_string(),
                "display_remoting".to_string(),
                "-b:v".to_string(),
                config.bitrate.to_string(),
                "-maxrate".to_string(),
                max_bitrate.to_string(),
                "-bufsize".to_string(),
                config.bitrate.to_string(),
                "-g".to_string(),
                config.fps.to_string(),
            ]);
        }
        CaptureEncoder::Software => {
            args.extend([
                "-vf".to_string(),
                format!(
                    "{clock_filter},hwdownload,format=bgra,scale={}:{}:flags=fast_bilinear,pad=ceil(iw/2)*2:ceil(ih/2)*2",
                    config.width, config.height
                ),
                "-c:v".to_string(),
                "libx264".to_string(),
                "-preset".to_string(),
                "ultrafast".to_string(),
                "-tune".to_string(),
                "zerolatency".to_string(),
                "-crf".to_string(),
                "18".to_string(),
                "-g".to_string(),
                config.fps.to_string(),
                "-sc_threshold".to_string(),
                "0".to_string(),
                "-pix_fmt".to_string(),
                "yuv420p".to_string(),
            ]);
        }
    }

    // Capture segments keep the moov atom at the end: a faststart rewrite of a
    // multi-GB segment on stop could outlast the stop timeout and be killed
    // mid-rewrite. The stitched output (concat_args) is the faststart file.
    args.extend([
        "-fps_mode".to_string(),
        "cfr".to_string(),
        output.to_string_lossy().to_string(),
    ]);
    args
}

fn stop_signal_context(error: Option<&str>) -> String {
    error
        .map(|error| format!("; stop pipe unavailable: {error}"))
        .unwrap_or_default()
}

fn is_progress_line(line: &str) -> bool {
    [
        "frame=",
        "fps=",
        "stream_",
        "bitrate=",
        "total_size=",
        "out_time_ms=",
        "out_time=",
        "dup_frames=",
        "drop_frames=",
        "speed=",
        "progress=",
    ]
    .iter()
    .any(|prefix| line.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_pipe_child() {
        if !std::env::args().any(|arg| arg == "capture-pipe-fixture") {
            return;
        }
        let mut command = String::new();
        std::io::stdin().read_line(&mut command).unwrap();
        assert_eq!(command, "q\n");
        eprintln!("out_time_us=1234000");
        eprintln!("capture-fixture-final-diagnostic");
        std::process::exit(0);
    }

    #[test]
    fn completed_capture_drains_final_progress_and_releases_its_launch_permit() {
        let _guard = TEST_LOCK.lock().unwrap();
        let name = format!(
            "{}::capture_pipe_child",
            module_path!().split_once("::").unwrap().1
        );
        let args = [
            "--exact",
            &name,
            "--nocapture",
            "--",
            "capture-pipe-fixture",
        ]
        .map(str::to_string);
        let mut process = ManagedCaptureProcess::spawn(&test_executable(), &args).unwrap();
        let outcome = process.stop_gracefully(Duration::from_secs(10)).unwrap();
        assert!(outcome.status.success());
        assert_eq!(outcome.encoded_duration, Duration::from_micros(1_234_000));
        assert!(
            outcome
                .diagnostics
                .contains("capture-fixture-final-diagnostic")
        );
        assert!(process.child.stopped());
        assert!(LAUNCH.acquire().is_err());
        drop(process);
        assert!(LAUNCH.acquire().is_ok());
    }

    fn config() -> CaptureProcessConfig {
        CaptureProcessConfig {
            monitor_index: 2,
            fps: 100,
            include_cursor: true,
            width: 2560,
            height: 1080,
            bitrate: 60_000_000,
        }
    }

    #[test]
    fn capture_reports_the_first_frame_epoch_before_normalizing_video_pts() {
        for encoder in [CaptureEncoder::MediaFoundation, CaptureEncoder::Software] {
            let args = build_ffmpeg_args(&config(), encoder, Path::new("capture.mp4"));
            let joined = args.join(" ");
            assert!(joined.contains("-copyts -use_wallclock_as_timestamps 1"));
            assert!(joined.contains("settb=1/1000000,showinfo=checksum=0,setpts=PTS-STARTPTS"));
        }
    }

    #[test]
    fn capture_segments_never_request_a_faststart_rewrite() {
        for encoder in [CaptureEncoder::MediaFoundation, CaptureEncoder::Software] {
            let args = build_ffmpeg_args(&config(), encoder, Path::new("segment.mp4"));
            assert!(
                !args.iter().any(|arg| arg.contains("faststart")),
                "{args:?}"
            );
        }
    }

    #[test]
    fn hardware_encoder_keeps_desktop_frames_on_the_gpu() {
        let args = build_ffmpeg_args(
            &config(),
            CaptureEncoder::MediaFoundation,
            Path::new("capture.mp4"),
        );
        let joined = args.join(" ");
        assert!(joined.contains("ddagrab=output_idx=2:framerate=100:draw_mouse=1"));
        assert!(joined.contains("-c:v h264_mf"));
        assert!(joined.contains("-hw_encoding 1"));
        assert!(!joined.contains("hwdownload"));
        assert!(!joined.contains("libx264"));
    }

    #[test]
    fn software_fallback_downloads_and_converts_frames() {
        let args = build_ffmpeg_args(
            &config(),
            CaptureEncoder::Software,
            Path::new("capture.mp4"),
        );
        let joined = args.join(" ");
        assert!(joined.contains("hwdownload,format=bgra"));
        assert!(joined.contains("scale=2560:1080"));
        assert!(joined.contains("-c:v libx264"));
        assert!(!joined.contains("h264_mf"));
    }
}
