use super::CHILD_FLAG;
use super::protocol::{ChildEvent, HostCommand, StatusSnapshot};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{LazyLock, Mutex, Once};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const HEARTBEAT_TIMEOUT_MS: u64 = 5_000;

struct RendererProcess {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    generation: u64,
}

enum Message {
    Command(HostCommand),
    Restart,
    Warmup,
}

pub(super) static SNAPSHOT: LazyLock<Mutex<StatusSnapshot>> = LazyLock::new(|| {
    Mutex::new(StatusSnapshot {
        is_dark: crate::overlay::is_dark_mode(),
        selection: super::protocol::SelectionScene {
            capture_visible: true,
            ..Default::default()
        },
        ..Default::default()
    })
});
static PROCESS: LazyLock<Mutex<Option<RendererProcess>>> = LazyLock::new(|| Mutex::new(None));
static SENDER: LazyLock<Sender<Message>> = LazyLock::new(|| {
    let (sender, receiver) = channel();
    std::thread::Builder::new()
        .name("sgt-status-delivery".to_string())
        .spawn(move || delivery_loop(receiver))
        .expect("failed to start status compositor delivery thread");
    sender
});
static STARTING: AtomicBool = AtomicBool::new(false);
static GENERATION: AtomicU64 = AtomicU64::new(0);
static LIVE_GENERATION: AtomicU64 = AtomicU64::new(0);
static READY_GENERATION: AtomicU64 = AtomicU64::new(0);
static LAST_HEARTBEAT_MS: AtomicU64 = AtomicU64::new(0);
static CAPTURE_APPLIED: AtomicU64 = AtomicU64::new(0);
static WATCHDOG: Once = Once::new();

pub(super) fn warmup() {
    start_watchdog();
    let _ = SENDER.send(Message::Warmup);
}

pub(super) fn send(command: HostCommand) {
    start_watchdog();
    let _ = SENDER.send(Message::Command(command));
}

pub(super) fn wait_for_capture(request_id: u64, timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if CAPTURE_APPLIED.load(Ordering::SeqCst) >= request_id {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    false
}

pub(super) fn wait_until_ready(timeout: Duration) -> bool {
    warmup();
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        let live = LIVE_GENERATION.load(Ordering::SeqCst);
        if live != 0 && READY_GENERATION.load(Ordering::SeqCst) == live {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

fn delivery_loop(receiver: Receiver<Message>) {
    while let Ok(message) = receiver.recv() {
        let mut commands = Vec::new();
        let mut restart = false;
        let mut warmup = false;
        collect(message, &mut commands, &mut restart, &mut warmup);
        while let Ok(message) = receiver.try_recv() {
            collect(message, &mut commands, &mut restart, &mut warmup);
        }
        let seeded_from_snapshot = if restart {
            restart_now()
        } else if warmup || !commands.is_empty() {
            ensure_process()
        } else {
            false
        };
        if seeded_from_snapshot {
            continue;
        }
        for command in coalesce(commands) {
            if write_command(&command).is_err() {
                restart_now();
                break;
            }
        }
    }
}

fn collect(
    message: Message,
    commands: &mut Vec<HostCommand>,
    restart: &mut bool,
    warmup: &mut bool,
) {
    match message {
        Message::Command(command) => commands.push(command),
        Message::Restart => *restart = true,
        Message::Warmup => *warmup = true,
    }
}

fn coalesce(commands: Vec<HostCommand>) -> Vec<HostCommand> {
    let mut output: Vec<Option<HostCommand>> = Vec::with_capacity(commands.len());
    let mut recording_update = None;
    let mut selection_position = None;
    let mut theme = None;
    for command in commands {
        let slot = match command {
            HostCommand::RecordingUpdate { .. } => &mut recording_update,
            HostCommand::SelectionPosition { .. } => &mut selection_position,
            HostCommand::Theme { .. } => &mut theme,
            other => {
                output.push(Some(other));
                continue;
            }
        };
        if let Some(index) = *slot {
            output[index] = Some(command);
        } else {
            *slot = Some(output.len());
            output.push(Some(command));
        }
    }
    output.into_iter().flatten().collect()
}

fn ensure_process() -> bool {
    if PROCESS.lock().unwrap().is_some() || STARTING.swap(true, Ordering::SeqCst) {
        return false;
    }
    let spawned = match spawn_process() {
        Ok(()) => true,
        Err(error) => {
            crate::log_info!("[StatusCompositor] failed to start renderer: {error:#}");
            false
        }
    };
    STARTING.store(false, Ordering::SeqCst);
    spawned
}

fn spawn_process() -> anyhow::Result<()> {
    let mut child = Command::new(std::env::current_exe()?)
        .arg(CHILD_FLAG)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("status renderer stdin was not created"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("status renderer stdout was not created"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("status renderer stderr was not created"))?;
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    LIVE_GENERATION.store(generation, Ordering::SeqCst);
    READY_GENERATION.store(0, Ordering::SeqCst);
    LAST_HEARTBEAT_MS.store(now_ms(), Ordering::SeqCst);
    *PROCESS.lock().unwrap() = Some(RendererProcess {
        child,
        stdin: BufWriter::new(stdin),
        generation,
    });
    std::thread::spawn(move || read_events(stdout, generation));
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            crate::log_info!("[StatusCompositor] child generation={generation}: {line}");
        }
    });
    crate::log_info!("[StatusCompositor] renderer spawned generation={generation}");
    write_command(&HostCommand::Snapshot {
        scene: SNAPSHOT.lock().unwrap().clone(),
    })?;
    Ok(())
}

fn write_command(command: &HostCommand) -> anyhow::Result<()> {
    let mut process = PROCESS.lock().unwrap();
    let renderer = process
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("status compositor is unavailable"))?;
    serde_json::to_writer(&mut renderer.stdin, command)?;
    renderer.stdin.write_all(b"\n")?;
    renderer.stdin.flush()?;
    Ok(())
}

fn read_events(stdout: std::process::ChildStdout, generation: u64) {
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        let Ok(event) = serde_json::from_str::<ChildEvent>(&line) else {
            crate::log_info!(
                "[StatusCompositor] invalid event generation={generation} bytes={}",
                line.len()
            );
            continue;
        };
        if LIVE_GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        match event {
            ChildEvent::Ready => {
                READY_GENERATION.store(generation, Ordering::SeqCst);
                LAST_HEARTBEAT_MS.store(now_ms(), Ordering::SeqCst);
                crate::log_info!("[StatusCompositor] renderer ready generation={generation}");
            }
            ChildEvent::Heartbeat => LAST_HEARTBEAT_MS.store(now_ms(), Ordering::SeqCst),
            ChildEvent::RecordingReady => crate::overlay::recording::compositor_ready(),
            ChildEvent::RecordingPauseToggle => {
                crate::overlay::recording::compositor_toggle_pause()
            }
            ChildEvent::RecordingCancel => crate::overlay::recording::compositor_cancel(),
            ChildEvent::RecordingMoved { rect } => {
                if let Some(recording) = SNAPSHOT.lock().unwrap().recording.as_mut() {
                    recording.rect = rect;
                }
            }
            ChildEvent::NotificationFinished { through_id } => {
                SNAPSHOT
                    .lock()
                    .unwrap()
                    .notifications
                    .retain(|notification| notification.id > through_id);
            }
            ChildEvent::SelectionCaptureApplied { request_id } => {
                CAPTURE_APPLIED.fetch_max(request_id, Ordering::SeqCst);
            }
            ChildEvent::RendererError { source, error } => {
                crate::log_info!("[StatusCompositor] renderer error source={source} error={error}")
            }
        }
    }
    if LIVE_GENERATION
        .compare_exchange(generation, 0, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        READY_GENERATION.store(0, Ordering::SeqCst);
        crate::log_info!("[StatusCompositor] renderer disconnected generation={generation}");
    }
}

fn start_watchdog() {
    WATCHDOG.call_once(|| {
        std::thread::spawn(|| {
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let live = LIVE_GENERATION.load(Ordering::SeqCst);
                if live == 0 {
                    let _ = SENDER.send(Message::Restart);
                    continue;
                }
                let timeout = if READY_GENERATION.load(Ordering::SeqCst) == live {
                    HEARTBEAT_TIMEOUT_MS
                } else {
                    15_000
                };
                if now_ms().saturating_sub(LAST_HEARTBEAT_MS.load(Ordering::SeqCst)) > timeout {
                    crate::log_info!("[StatusCompositor] heartbeat timed out; restarting");
                    let _ = SENDER.send(Message::Restart);
                }
            }
        });
    });
}

fn restart_now() -> bool {
    let mut old = PROCESS.lock().unwrap().take();
    if let Some(renderer) = old.as_mut() {
        LIVE_GENERATION
            .compare_exchange(renderer.generation, 0, Ordering::SeqCst, Ordering::SeqCst)
            .ok();
        READY_GENERATION.store(0, Ordering::SeqCst);
        let _ = renderer.child.kill();
        let _ = renderer.child.wait();
    }
    drop(old);
    ensure_process()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::status_compositor::protocol::PhysicalRect;

    #[test]
    fn high_frequency_updates_keep_only_the_latest_value() {
        let commands = coalesce(vec![
            HostCommand::RecordingUpdate {
                state: "warmup".to_string(),
                rms: 0.1,
            },
            HostCommand::SelectionPosition {
                rect: PhysicalRect::default(),
            },
            HostCommand::RecordingUpdate {
                state: "recording".to_string(),
                rms: 0.8,
            },
        ]);
        assert_eq!(commands.len(), 2);
        assert!(
            matches!(&commands[0], HostCommand::RecordingUpdate { state, .. } if state == "recording")
        );
    }
}
