//! Coordinates a recorder window's durable save before its worker exits.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex, mpsc};
use std::time::Duration;

use anyhow::{Result, anyhow, bail};

const PREPARE_TIMEOUT: Duration = Duration::from_secs(45);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
// Even generations admit starts; odd generations mean cleanup is underway.
static RECORDING_GENERATION: AtomicU64 = AtomicU64::new(0);
static PREPARE_OWNER: AtomicU64 = AtomicU64::new(0);
static COMMITTED_CLOSE_OWNER: AtomicU64 = AtomicU64::new(0);
static RECORDING_CLEANUP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
// High bits count cleanups; bit zero records a later show request. One atomic
// transition makes a new cleanup supersede an older show without an admission gap.
static ACTIVE_RECORDING_CLEANUPS: AtomicUsize = AtomicUsize::new(0);
const PENDING_SHOW: usize = 1;
const ONE_CLEANUP: usize = 2;
static INCLUDE_CURSOR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RECORDING: LazyLock<Mutex<RecordingReceipt>> =
    LazyLock::new(|| Mutex::new(RecordingReceipt::recover()));
type ShutdownRequest = Option<(u64, mpsc::SyncSender<bool>)>;
static PENDING: LazyLock<Mutex<ShutdownRequest>> = LazyLock::new(|| Mutex::new(None));

#[derive(Clone, Default)]
struct RecordingReceipt {
    pending: Option<serde_json::Value>,
    last_saved: Option<String>,
    failure_id: u64,
    last_released: Option<u64>,
    interrupted: Option<super::recording_journal::Journal>,
    recovery_error: Option<String>,
    recovered_from_previous_worker: bool,
}

struct RecordingCleanupActivity;

impl RecordingCleanupActivity {
    fn begin() -> Self {
        let _ =
            ACTIVE_RECORDING_CLEANUPS.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |state| {
                Some((state & !PENDING_SHOW) + ONE_CLEANUP)
            });
        Self
    }
}

impl Drop for RecordingCleanupActivity {
    fn drop(&mut self) {
        let _receipt = RECORDING.lock().unwrap_or_else(|error| error.into_inner());
        let previous = ACTIVE_RECORDING_CLEANUPS.fetch_sub(ONE_CLEANUP, Ordering::SeqCst);
        if previous == ONE_CLEANUP + PENDING_SHOW
            && PREPARE_OWNER.load(Ordering::SeqCst) == 0
            && !std::thread::panicking()
        {
            ACTIVE_RECORDING_CLEANUPS.fetch_and(!PENDING_SHOW, Ordering::SeqCst);
            RECORDING_CLEANUP.store(false, Ordering::SeqCst);
            set_recording_admission(&RECORDING_GENERATION, true);
        }
    }
}

pub(super) fn with_recording_cleanup(cleanup: impl FnOnce()) {
    let _activity = RecordingCleanupActivity::begin();
    RECORDING_CLEANUP.store(true, Ordering::SeqCst);
    set_recording_admission(&RECORDING_GENERATION, false);
    // Cleanup must wait for a start or stop to finish installing its session.
    let _receipt = RECORDING.lock().unwrap_or_else(|error| error.into_inner());
    RECORDING_CLEANUP.store(true, Ordering::SeqCst);
    set_recording_admission(&RECORDING_GENERATION, false);
    cleanup();
}

fn set_recording_admission(generation: &AtomicU64, open: bool) {
    let _ = generation.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
        (current.is_multiple_of(2) != open).then(|| current.wrapping_add(1))
    });
}

pub(super) fn reopen_recording_admission() {
    let _receipt = RECORDING.lock().unwrap_or_else(|error| error.into_inner());
    if PREPARE_OWNER.load(Ordering::SeqCst) == 0
        && ACTIVE_RECORDING_CLEANUPS.load(Ordering::SeqCst) & !PENDING_SHOW == 0
    {
        ACTIVE_RECORDING_CLEANUPS.fetch_and(!PENDING_SHOW, Ordering::SeqCst);
        RECORDING_CLEANUP.store(false, Ordering::SeqCst);
        set_recording_admission(&RECORDING_GENERATION, true);
    } else {
        ACTIVE_RECORDING_CLEANUPS.fetch_or(PENDING_SHOW, Ordering::SeqCst);
    }
}

fn close_recording_admission(owner: u64) -> bool {
    let _receipt = RECORDING.lock().unwrap_or_else(|error| error.into_inner());
    if PREPARE_OWNER.load(Ordering::SeqCst) != owner {
        return false;
    }
    set_recording_admission(&RECORDING_GENERATION, false);
    true
}

pub(super) fn recording_generation() -> u64 {
    RECORDING_GENERATION.load(Ordering::SeqCst)
}

impl RecordingReceipt {
    fn recover() -> Self {
        match super::recording_journal::read() {
            Ok(Some(mut journal)) => {
                let pending = journal.result.take();
                Self {
                    failure_id: journal.id,
                    interrupted: pending.is_none().then_some(journal),
                    pending,
                    recovered_from_previous_worker: true,
                    ..Self::default()
                }
            }
            Ok(None) => Self::default(),
            Err(error) => Self {
                recovery_error: Some(error),
                ..Self::default()
            },
        }
    }
    fn recovery(&self, active: bool, include_cursor: bool) -> serde_json::Value {
        self.pending.clone().unwrap_or_else(|| {
            if active {
                serde_json::json!({"recordingActive": true, "recordingMode": if include_cursor { "withCursor" } else { "withoutCursor" }})
            } else {
                serde_json::Value::Null
            }
        })
    }

    fn can_release_interrupted(&self, capture_finished: bool) -> bool {
        self.recovered_from_previous_worker || capture_finished
    }

    fn acknowledge(&mut self, id: &str) -> Result<(), String> {
        if self.last_saved.as_deref() == Some(id) {
            return Ok(());
        }
        if self
            .pending
            .as_ref()
            .and_then(|value| value["recordingId"].as_str())
            != Some(id)
        {
            return Err("Stale recording identity".to_string());
        }
        self.pending = None;
        self.last_saved = Some(id.to_string());
        Ok(())
    }
}

pub(super) fn start_recording(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    start_recording_with_generation(args, recording_generation())
}

pub(super) fn start_recording_with_generation(
    args: &serde_json::Value,
    generation: u64,
) -> Result<serde_json::Value, String> {
    let mut receipt = RECORDING
        .lock()
        .map_err(|_| "Recording state unavailable")?;
    if PREPARE_OWNER.load(Ordering::SeqCst) != 0
        || ACTIVE_RECORDING_CLEANUPS.load(Ordering::SeqCst) & !PENDING_SHOW != 0
        || RECORDING_CLEANUP.load(Ordering::SeqCst)
        || !generation.is_multiple_of(2)
        || generation != recording_generation()
    {
        return Err("Recorder is closing; reopen it before starting a recording".to_string());
    }
    if receipt.pending.is_some() {
        return Err("A recording is still waiting to be saved".to_string());
    }
    if receipt.interrupted.is_some() || receipt.recovery_error.is_some() {
        return Err("An interrupted recording still needs attention".into());
    }
    if super::engine::IS_RECORDING.load(Ordering::SeqCst) {
        return Err("A recording is already active".to_string());
    }
    if super::engine::FINALIZATION_ERROR.lock().is_some() {
        return Err("A failed recording still needs attention".to_string());
    }
    receipt.failure_id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    receipt.recovered_from_previous_worker = false;
    super::recording_journal::begin(receipt.failure_id)?;
    let result = match super::ipc::start_recording(args) {
        Ok(result) => result,
        Err(error) => {
            receipt.interrupted = super::recording_journal::failed_start(&error)?;
            return Err(error);
        }
    };
    INCLUDE_CURSOR.store(
        args["includeCursor"].as_bool().unwrap_or(false),
        Ordering::SeqCst,
    );
    receipt.last_saved = None;
    Ok(result)
}

pub(super) fn stop_recording() -> Result<serde_json::Value, String> {
    let mut receipt = RECORDING
        .lock()
        .map_err(|_| "Recording state unavailable")?;
    if let Some(result) = &receipt.pending {
        super::recording_journal::ready(result)?;
        return super::recording_journal::refresh_media_urls(result);
    }
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random).map_err(|error| format!("Cannot identify recording: {error}"))?;
    let id = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let mut result = super::ipc::stop_recording()?;
    let Some(object) = result.as_object_mut() else {
        *super::engine::FINALIZATION_ERROR.lock() =
            Some("Invalid recording result; recording files were retained".to_string());
        return Err("Invalid recording result; recording files were retained".to_string());
    };
    object.insert("recordingId".into(), id.into());
    object.insert(
        "recordingMode".into(),
        if INCLUDE_CURSOR.load(Ordering::SeqCst) {
            "withCursor"
        } else {
            "withoutCursor"
        }
        .into(),
    );
    receipt.pending = Some(result.clone());
    super::recording_journal::ready(&result)?;
    Ok(result)
}

pub(super) fn pending_recording() -> Result<serde_json::Value, String> {
    let receipt = RECORDING
        .lock()
        .map_err(|_| "Recording state unavailable")?;
    if let Some(result) = &receipt.pending {
        return super::recording_journal::refresh_media_urls(result);
    }
    Ok(receipt.recovery(
        super::engine::IS_RECORDING.load(Ordering::SeqCst),
        INCLUDE_CURSOR.load(Ordering::SeqCst),
    ))
}

pub(super) fn recording_saved(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let id = args["recordingId"]
        .as_str()
        .filter(|id| id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or("Missing recording identity")?;
    let mut receipt = RECORDING
        .lock()
        .map_err(|_| "Recording state unavailable")?;
    let mut next = receipt.clone();
    next.acknowledge(id)?;
    super::recording_journal::clear()?;
    *receipt = next;
    Ok(serde_json::Value::Null)
}

fn has_pending_recording() -> bool {
    RECORDING
        .lock()
        .map(|receipt| receipt.pending.is_some())
        .unwrap_or(true)
}

fn recording_finished() -> bool {
    [
        &*super::engine::ENCODING_FINISHED,
        &*super::engine::AUDIO_ENCODING_FINISHED,
        &*super::engine::MIC_AUDIO_ENCODING_FINISHED,
        &*super::engine::WEBCAM_ENCODING_FINISHED,
    ]
    .into_iter()
    .all(|flag| flag.load(Ordering::SeqCst))
}

pub(super) fn failed_recording() -> Result<serde_json::Value, String> {
    let receipt = RECORDING
        .lock()
        .map_err(|_| "Recording state unavailable")?;
    if let Some(error) = &receipt.recovery_error {
        return Ok(
            serde_json::json!({"id":0,"error":format!("Recording journal needs attention: {error}"),"directory":crate::paths::app_runtime_local_data_dir().join("recordings"),"paths":[],"canRelease":false}),
        );
    }
    if let Some(journal) = &receipt.interrupted {
        return Ok(
            serde_json::json!({"id":journal.id,"error":journal.error.as_deref().unwrap_or("Recording worker stopped before finalization; its files were retained"),"directory":crate::paths::app_runtime_local_data_dir().join("recordings"),"paths":journal.paths.iter().filter(|p|p.exists()).collect::<Vec<_>>(),"canRelease":receipt.can_release_interrupted(recording_finished())}),
        );
    }
    let error = super::engine::FINALIZATION_ERROR.lock().clone();
    let Some(error) = error else {
        return Ok(serde_json::Value::Null);
    };
    let directory = crate::paths::app_runtime_local_data_dir().join("recordings");
    let paths = [
        &super::engine::VIDEO_PATH,
        &super::engine::types::AUDIO_PATH,
        &super::engine::MIC_AUDIO_PATH,
        &super::engine::WEBCAM_VIDEO_PATH,
    ]
    .into_iter()
    .filter_map(|path| path.lock().ok().and_then(|value| value.clone()))
    .collect::<Vec<_>>();
    Ok(
        serde_json::json!({"id":receipt.failure_id,"error":error,"directory":directory,"paths":paths,"canRelease":recording_finished()}),
    )
}

pub(super) fn release_failed_recording(
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let id = args["id"].as_u64().ok_or("Missing recording identity")?;
    let mut receipt = RECORDING
        .lock()
        .map_err(|_| "Recording state unavailable")?;
    if receipt.last_released == Some(id) {
        return Ok(serde_json::Value::Null);
    }
    if receipt.failure_id != id {
        return Err("Stale recording identity".to_string());
    }
    if receipt.pending.is_some() {
        return Err("A saved recording is awaiting confirmation".to_string());
    }
    if receipt.interrupted.is_some() && receipt.can_release_interrupted(recording_finished()) {
        super::recording_journal::clear()?;
        receipt.interrupted = None;
        receipt.last_released = Some(id);
        *super::engine::FINALIZATION_ERROR.lock() = None;
        return Ok(serde_json::Value::Null);
    }
    let mut failure = super::engine::FINALIZATION_ERROR.lock();
    if failure.is_none() || !recording_finished() {
        return Err("Recording files are still being finalized".to_string());
    }
    super::engine::IS_RECORDING.store(false, Ordering::SeqCst);
    super::recording_journal::clear()?;
    *failure = None;
    receipt.last_released = Some(id);
    Ok(serde_json::Value::Null)
}

pub(super) fn prepare(override_id: usize) -> Result<u64> {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    if PREPARE_OWNER
        .compare_exchange(0, id, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        bail!("recorder shutdown is already in progress");
    }
    let result = if close_recording_admission(id) {
        prepare_owned(id, override_id)
    } else {
        Err(anyhow!("recorder shutdown was cancelled"))
    };
    if result.is_err() {
        cancel_owned(id);
    }
    result.map(|()| id)
}

fn prepare_owned(id: u64, override_id: usize) -> Result<()> {
    if let Some(error) = super::engine::FINALIZATION_ERROR.lock().clone() {
        bail!("recording was not finalized: {error}");
    }
    let (send, receive) = mpsc::sync_channel(1);
    {
        let mut pending = PENDING
            .lock()
            .map_err(|_| anyhow!("recorder shutdown lock poisoned"))?;
        if PREPARE_OWNER.load(Ordering::SeqCst) != id {
            bail!("recorder shutdown was cancelled");
        }
        if pending.is_some() {
            bail!("recorder shutdown is already in progress");
        }
        *pending = Some((id, send));
    }
    let posted = super::post_script(format!(
        "if(window.__srShutdownReady){{window.dispatchEvent(new CustomEvent('sr-prepare-shutdown',{{detail:{{requestId:{id},subtitleOverrideId:{override_id}}}}}));}}else{{window.invoke?.('shutdown_ack',{{requestId:{id},saved:false}});}}"
    ));
    let result = if !posted {
        Err(anyhow!(
            "recorder work cannot be checked without its editor"
        ))
    } else {
        match receive.recv_timeout(PREPARE_TIMEOUT) {
            Ok(true)
                if !super::engine::IS_RECORDING.load(Ordering::SeqCst)
                    && !has_pending_recording()
                    && PREPARE_OWNER.load(Ordering::SeqCst) == id =>
            {
                Ok(())
            }
            Ok(_) => Err(anyhow!("recording was not saved")),
            Err(_) => Err(anyhow!("recorder save did not finish in time")),
        }
    };
    if let Ok(mut pending) = PENDING.lock()
        && pending
            .as_ref()
            .is_some_and(|(request_id, _)| *request_id == id)
    {
        pending.take();
    }
    result
}

pub(super) fn cancel() {
    cancel_owned(PREPARE_OWNER.load(Ordering::SeqCst));
}

pub(super) fn cancel_prepared_owner(owner: u64) {
    cancel_owned(owner);
}

/// Commit the window destruction only if this prepare still owns admission.
/// Once committed, cancellation cannot reopen recording until the window ends.
pub(super) fn commit_prepared_owner(owner: u64) -> bool {
    let _receipt = RECORDING.lock().unwrap_or_else(|error| error.into_inner());
    if owner == 0
        || PREPARE_OWNER.load(Ordering::SeqCst) != owner
        || COMMITTED_CLOSE_OWNER.load(Ordering::SeqCst) != 0
    {
        return false;
    }
    COMMITTED_CLOSE_OWNER.store(owner, Ordering::SeqCst);
    true
}

pub(super) fn release_closed_window_owner(owner: u64) {
    {
        let _receipt = RECORDING.lock().unwrap_or_else(|error| error.into_inner());
        if COMMITTED_CLOSE_OWNER.load(Ordering::SeqCst) == owner {
            COMMITTED_CLOSE_OWNER.store(0, Ordering::SeqCst);
        }
    }
    cancel_owned(owner);
}

fn cancel_owned(owner: u64) {
    {
        let _receipt = RECORDING.lock().unwrap_or_else(|error| error.into_inner());
        if owner == 0
            || COMMITTED_CLOSE_OWNER.load(Ordering::SeqCst) == owner
            || PREPARE_OWNER
                .compare_exchange(owner, 0, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
        {
            return;
        }
        // Invalidate starts queued during this attempt, even if cancellation
        // won the race before the prepare thread acquired the receipt lock.
        set_recording_admission(&RECORDING_GENERATION, false);
        let activity = ACTIVE_RECORDING_CLEANUPS.load(Ordering::SeqCst);
        if activity & !PENDING_SHOW == 0
            && (!RECORDING_CLEANUP.load(Ordering::SeqCst) || activity & PENDING_SHOW != 0)
        {
            ACTIVE_RECORDING_CLEANUPS.fetch_and(!PENDING_SHOW, Ordering::SeqCst);
            RECORDING_CLEANUP.store(false, Ordering::SeqCst);
            set_recording_admission(&RECORDING_GENERATION, true);
        }
    }
    if let Ok(mut pending) = PENDING.lock()
        && pending.as_ref().is_some_and(|(id, _)| *id == owner)
        && let Some((_, sender)) = pending.take()
    {
        let _ = sender.try_send(false);
    }
    let _ = super::post_script(format!(
        "window.dispatchEvent(new CustomEvent('sr-cancel-shutdown',{{detail:{{requestId:{owner}}}}}));"
    ));
}

pub(super) fn acknowledge(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let id = args["requestId"]
        .as_u64()
        .ok_or("Missing shutdown request")?;
    let saved = args["saved"].as_bool().ok_or("Missing shutdown result")?;
    let mut pending = PENDING
        .lock()
        .map_err(|_| "Recorder shutdown unavailable")?;
    let Some((expected, sender)) = pending.as_ref() else {
        return Err("Recorder shutdown request is no longer active".to_string());
    };
    if *expected != id {
        return Err("Stale recorder shutdown request".to_string());
    }
    sender
        .try_send(saved)
        .map_err(|_| "Recorder shutdown response unavailable")?;
    pending.take();
    Ok(serde_json::Value::Null)
}

#[cfg(test)]
#[path = "shutdown_tests.rs"]
mod tests;
