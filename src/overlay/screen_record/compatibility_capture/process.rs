use anyhow::{Context, Result, bail};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::ffi::c_void;
use std::io::{BufRead, BufReader, Write};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject,
};
use windows::core::PCWSTR;

const FFMPEG_ENV: &str = "SGT_FFMPEG_PATH";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const STARTUP_GRACE: Duration = Duration::from_millis(400);
const STDERR_TAIL_LINES: usize = 32;

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
    child: Child,
    stdin: Option<ChildStdin>,
    stderr_thread: Option<JoinHandle<()>>,
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
    progress_us: Arc<AtomicU64>,
    job: OwnedHandle,
}

impl ManagedCaptureProcess {
    pub(super) fn spawn(ffmpeg: &Path, args: &[String]) -> Result<Self> {
        let mut command = Command::new(ffmpeg);
        command
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW);

        let mut child = command
            .spawn()
            .with_context(|| format!("start FFmpeg at {}", ffmpeg.display()))?;
        let job = match create_kill_on_close_job(&child) {
            Ok(job) => job,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        let stdin = child.stdin.take();
        let stderr = child.stderr.take().context("open FFmpeg stderr")?;
        let stderr_tail = Arc::new(Mutex::new(VecDeque::new()));
        let reader_tail = stderr_tail.clone();
        let progress_us = Arc::new(AtomicU64::new(0));
        let reader_progress_us = progress_us.clone();
        let stderr_thread = std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(std::result::Result::ok) {
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
            job,
        })
    }

    pub(super) fn verify_started(&mut self) -> Result<()> {
        std::thread::sleep(STARTUP_GRACE);
        if let Some(outcome) = self.poll_exit()? {
            bail!(
                "FFmpeg compatibility capture exited during startup ({}): {}",
                outcome.status,
                outcome.diagnostics
            );
        }
        Ok(())
    }

    pub(super) fn poll_exit(&mut self) -> Result<Option<ProcessOutcome>> {
        let Some(status) = self.child.try_wait().context("poll FFmpeg exit")? else {
            return Ok(None);
        };
        self.join_stderr();
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
                let _ = unsafe { TerminateJobObject(job_handle(&self.job), 1) };
                let status = self.child.wait().context("wait for terminated FFmpeg")?;
                self.join_stderr();
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
                let _ = unsafe { TerminateJobObject(job_handle(&self.job), 1) };
                let status = self.child.wait().context("wait for terminated FFmpeg")?;
                self.join_stderr();
                bail!(
                    "FFmpeg process exceeded {} ms ({status}): {}",
                    timeout.as_millis(),
                    self.stderr_summary()
                );
            }
            std::thread::sleep(Duration::from_millis(40));
        }
    }

    fn join_stderr(&mut self) {
        if let Some(thread) = self.stderr_thread.take() {
            let _ = thread.join();
        }
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
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = unsafe { TerminateJobObject(job_handle(&self.job), 1) };
            let _ = self.child.wait();
        }
        self.join_stderr();
    }
}

pub(super) fn resolve_ffmpeg() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os(FFMPEG_ENV).map(PathBuf::from) {
        if path.is_file() {
            return Ok(path);
        }
        bail!("{FFMPEG_ENV} does not point to a file: {}", path.display());
    }

    let managed = crate::gui::settings_ui::download_manager::ffmpeg_dependency::ffmpeg_exe_path();
    if managed.is_file() {
        return Ok(managed);
    }

    let path_candidate = PathBuf::from("ffmpeg.exe");
    let status = Command::new(&path_candidate)
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status();
    if status.is_ok_and(|status| status.success()) {
        return Ok(path_candidate);
    }

    bail!(
        "FFmpeg is required for compatibility capture; install it in Downloaded Tools or set {FFMPEG_ENV}"
    )
}

pub(super) fn build_ffmpeg_args(
    config: &CaptureProcessConfig,
    encoder: CaptureEncoder,
    output: &Path,
) -> Vec<String> {
    let mut args = vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "warning".to_string(),
        "-nostats".to_string(),
        "-progress".to_string(),
        "pipe:2".to_string(),
        "-stats_period".to_string(),
        "0.25".to_string(),
        "-y".to_string(),
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

    match encoder {
        CaptureEncoder::MediaFoundation => {
            let max_bitrate = u64::from(config.bitrate)
                .saturating_mul(5)
                .div_ceil(4)
                .min(100_000_000);
            args.extend([
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
                    "hwdownload,format=bgra,scale={}:{}:flags=fast_bilinear,pad=ceil(iw/2)*2:ceil(ih/2)*2",
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

    args.extend([
        "-fps_mode".to_string(),
        "cfr".to_string(),
        "-movflags".to_string(),
        "+faststart".to_string(),
        output.to_string_lossy().to_string(),
    ]);
    args
}

fn create_kill_on_close_job(child: &Child) -> Result<OwnedHandle> {
    let raw_job =
        unsafe { CreateJobObjectW(None, PCWSTR::null()) }.context("create capture process job")?;
    let job = unsafe { OwnedHandle::from_raw_handle(raw_job.0) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    unsafe {
        SetInformationJobObject(
            job_handle(&job),
            JobObjectExtendedLimitInformation,
            &limits as *const _ as *const c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
        .context("configure capture process job")?;
        AssignProcessToJobObject(job_handle(&job), HANDLE(child.as_raw_handle()))
            .context("contain FFmpeg in capture process job")?;
    }
    Ok(job)
}

fn job_handle(job: &OwnedHandle) -> HANDLE {
    HANDLE(job.as_raw_handle())
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
