//! Authenticated worker-to-host settings requests.

use std::collections::HashMap;
use std::io::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex, OnceLock, mpsc};
use std::time::Duration;

use anyhow::{Context, Result};
use sgt_recorder_protocol::{
    RecorderSettings, SETTINGS_PREFIX, SETTINGS_PROTOCOL_VERSION, SettingsChange, SettingsEvent,
};

type Pending = mpsc::SyncSender<Result<RecorderSettings, String>>;
static TOKEN: OnceLock<String> = OnceLock::new();
static NEXT_EVENT: AtomicU64 = AtomicU64::new(1);
static PENDING: LazyLock<Mutex<HashMap<u64, Pending>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static LAST_APPLIED: Mutex<u64> = Mutex::new(0);
const SETTINGS_TIMEOUT: Duration = Duration::from_secs(15);

pub(super) fn initialize(token: String) -> Result<()> {
    TOKEN
        .set(token)
        .map_err(|_| anyhow::anyhow!("recorder settings already initialized"))
}

pub(super) fn request(change: SettingsChange) -> Result<RecorderSettings, String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let event_id =
        send(change, Some(sender)).map_err(|_| "SGT_SETTINGS_OUTCOME_UNKNOWN".to_string())?;
    let response = receive_response(&receiver, SETTINGS_TIMEOUT);
    PENDING.lock().unwrap().remove(&event_id);
    response
}

fn receive_response(
    receiver: &mpsc::Receiver<Result<RecorderSettings, String>>,
    timeout: Duration,
) -> Result<RecorderSettings, String> {
    receiver
        .recv_timeout(timeout)
        .map_err(|_| "SGT_SETTINGS_OUTCOME_UNKNOWN".to_string())?
}

pub(super) fn notify(change: SettingsChange) -> Result<()> {
    send(change, None).map(|_| ())
}

fn send(change: SettingsChange, pending: Option<Pending>) -> Result<u64> {
    let token = TOKEN
        .get()
        .context("recorder settings channel is not initialized")?;
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    let event_id = NEXT_EVENT.fetch_add(1, Ordering::Relaxed);
    let event = SettingsEvent {
        protocol_version: SETTINGS_PROTOCOL_VERSION,
        token: token.clone(),
        event_id,
        change,
    };
    let body = serde_json::to_vec(&event)?;
    if let Some(sender) = pending {
        PENDING.lock().unwrap().insert(event_id, sender);
    }
    let mut line = Vec::with_capacity(SETTINGS_PREFIX.len() + body.len() + 1);
    line.extend_from_slice(SETTINGS_PREFIX.as_bytes());
    line.extend_from_slice(&body);
    line.push(b'\n');
    if let Err(error) = output.write_all(&line).and_then(|_| output.flush()) {
        PENDING.lock().unwrap().remove(&event_id);
        return Err(error.into());
    }
    Ok(event_id)
}

pub(super) fn acknowledge(
    event_id: u64,
    settings: Option<RecorderSettings>,
    error: Option<String>,
) {
    if let Some(settings) = &settings {
        update_local(event_id, settings);
    }
    if let Some(sender) = PENDING.lock().unwrap().remove(&event_id) {
        let result = match (settings, error) {
            (Some(settings), None) => Ok(settings),
            (None, Some(error)) => Err(error),
            _ => Err("Invalid recorder settings response".to_string()),
        };
        let _ = sender.send(result);
    }
}

fn update_local(event_id: u64, settings: &RecorderSettings) {
    let mut last_applied = LAST_APPLIED.lock().unwrap();
    if event_id <= *last_applied {
        return;
    }
    let mut app = crate::APP.lock().unwrap();
    app.config.screen_record_hotkeys = settings
        .hotkeys
        .iter()
        .map(|h| crate::config::Hotkey::new(h.code, h.modifiers))
        .collect();
    app.config.max_screen_record_projects = settings.project_limit;
    app.config.max_screen_record_recent_uploads = settings.upload_limit;
    app.config.screen_record_window_size = settings.window_size;
    *last_applied = event_id;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_acknowledgement_is_unknown_but_a_host_error_is_definite() {
        let (sender, receiver) = mpsc::sync_channel(1);
        assert_eq!(
            receive_response(&receiver, Duration::from_millis(1)).unwrap_err(),
            "SGT_SETTINGS_OUTCOME_UNKNOWN"
        );
        sender.send(Err("disk full".to_string())).unwrap();
        assert_eq!(
            receive_response(&receiver, Duration::from_secs(1)).unwrap_err(),
            "disk full"
        );
        drop(sender);
        assert_eq!(
            receive_response(&receiver, Duration::from_secs(1)).unwrap_err(),
            "SGT_SETTINGS_OUTCOME_UNKNOWN"
        );
    }
}
