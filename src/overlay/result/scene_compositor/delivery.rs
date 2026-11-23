use super::mailbox::{CommandBuffer, PushResult, survives_snapshot};
use super::protocol::HostCommand;
use super::supervisor::{ProcessState, ensure_process, restart_now, start_watchdog, write_command};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{LazyLock, Mutex, MutexGuard};

#[derive(Default)]
struct PendingDelivery {
    commands: CommandBuffer,
    restart: bool,
    warmup: bool,
    snapshot_required: bool,
}

struct DeliveryBatch {
    commands: Vec<HostCommand>,
    restart: bool,
    warmup: bool,
}

static PENDING: LazyLock<Mutex<PendingDelivery>> =
    LazyLock::new(|| Mutex::new(PendingDelivery::default()));
static SIGNAL: LazyLock<SyncSender<()>> = LazyLock::new(|| {
    let (sender, receiver) = sync_channel(1);
    std::thread::Builder::new()
        .name("sgt-result-delivery".to_string())
        .spawn(move || delivery_loop(receiver))
        .expect("failed to start result delivery thread");
    sender
});

pub(super) fn warmup() {
    start_watchdog();
    PENDING.lock().unwrap().warmup = true;
    signal_delivery();
}

pub(super) fn send_command(command: HostCommand) {
    start_watchdog();
    let mut pending = PENDING.lock().unwrap();
    if pending.commands.push(command) != PushResult::Queued {
        pending.snapshot_required = true;
    }
    drop(pending);
    signal_delivery();
}

pub(super) fn request_restart() {
    PENDING.lock().unwrap().restart = true;
    signal_delivery();
}

pub(super) fn queue_snapshot() {
    let (mut pending, language, theme_mode) = pending_with_config();
    pending
        .commands
        .replace_with_snapshot(super::parent::scene_snapshot());
    pending
        .commands
        .push(super::localization::command_for(&language));
    pending.commands.push(super::processing::snapshot());
    pending.commands.push(super::capture_exclusion::snapshot());
    pending.commands.push(HostCommand::Theme {
        theme: super::parent::theme_command(theme_mode.is_dark()),
    });
    pending.snapshot_required = false;
    drop(pending);
    signal_delivery();
}

fn delivery_loop(receiver: Receiver<()>) {
    while receiver.recv().is_ok() {
        while receiver.try_recv().is_ok() {}
        let batch = take_pending();
        if batch.restart {
            deliver_batch(restart_now(), batch.commands);
            continue;
        }
        let process = if batch.warmup || !batch.commands.is_empty() {
            ensure_process()
        } else {
            ProcessState::Unavailable
        };
        deliver_batch(process, batch.commands);
    }
}

fn deliver_batch(process: ProcessState, mut commands: Vec<HostCommand>) {
    match process {
        ProcessState::Running => {}
        ProcessState::Spawned => commands.retain(survives_snapshot),
        ProcessState::Unavailable => {
            defer_operations(commands);
            return;
        }
    }
    if !commands.is_empty() {
        let rev = super::reconciliation::advance_revision();
        commands.push(HostCommand::ApplyRevision { revision: rev });
    }
    for command in commands {
        if let Err(error) = write_command(&command) {
            crate::log_info!("[ResultCompositor] command delivery failed: {error:#}");
            super::supervisor::fail_live_renderer("command delivery failed", true);
            break;
        }
    }
}

fn defer_operations(commands: Vec<HostCommand>) {
    let mut pending = PENDING.lock().unwrap();
    for command in commands.into_iter().filter(survives_snapshot) {
        if pending.commands.push(command) != PushResult::Queued {
            pending.snapshot_required = true;
        }
    }
}

fn take_pending() -> DeliveryBatch {
    let (mut pending, language, theme_mode) = pending_with_config();
    if pending.snapshot_required {
        pending
            .commands
            .replace_with_snapshot(super::parent::scene_snapshot());
        pending
            .commands
            .push(super::localization::command_for(&language));
        pending.commands.push(super::processing::snapshot());
        pending.commands.push(super::capture_exclusion::snapshot());
        pending.commands.push(HostCommand::Theme {
            theme: super::parent::theme_command(theme_mode.is_dark()),
        });
        pending.snapshot_required = false;
    }
    DeliveryBatch {
        commands: pending.commands.drain(),
        restart: std::mem::take(&mut pending.restart),
        warmup: std::mem::take(&mut pending.warmup),
    }
}

fn signal_delivery() {
    let _ = SIGNAL.try_send(());
}

// Keep config-to-queue lock ordering: recovery must not replace a newly queued
// locale with a snapshot captured before that config change.
fn pending_with_config() -> (
    MutexGuard<'static, PendingDelivery>,
    String,
    crate::config::ThemeMode,
) {
    let app = crate::APP.lock().unwrap();
    let pending = PENDING.lock().unwrap();
    (
        pending,
        app.config.ui_language.clone(),
        app.config.theme_mode.clone(),
    )
}

pub(crate) fn update_locale() {
    start_watchdog();
    let (mut pending, language, _) = pending_with_config();
    if pending
        .commands
        .push(super::localization::command_for(&language))
        != PushResult::Queued
    {
        pending.snapshot_required = true;
    }
    drop(pending);
    signal_delivery();
}
