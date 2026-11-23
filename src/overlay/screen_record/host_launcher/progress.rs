//! Session-owned projection of worker progress into the host registry.

use std::collections::{HashMap, HashSet};
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sgt_recorder_protocol::{MAX_PROGRESS_DATAGRAM_BYTES, ProgressEntry, ProgressEvent};

#[derive(Default)]
pub(super) struct ProgressSession {
    state: Mutex<State>,
}

struct RemoteEntry {
    guard: crate::install_progress::Guard,
    key: String,
    percent: Option<f32>,
    revision: u64,
}

#[derive(Default)]
struct State {
    retired: bool,
    highest_id: u64,
    entries: HashMap<u64, RemoteEntry>,
}

impl ProgressSession {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub(super) fn bind() -> Option<UdpSocket> {
        let socket = UdpSocket::bind("127.0.0.1:0").ok()?;
        socket
            .set_read_timeout(Some(Duration::from_millis(250)))
            .ok()?;
        Some(socket)
    }

    pub(super) fn start(self: &Arc<Self>, socket: UdpSocket, token: String) {
        let session = self.clone();
        let _ = std::thread::Builder::new()
            .name("recorder-progress-reader".into())
            .spawn(move || {
                let mut buffer = vec![0_u8; MAX_PROGRESS_DATAGRAM_BYTES + 1];
                let mut previous = 0;
                let mut last_seen = Instant::now();
                while !session.is_retired() {
                    match socket.recv_from(&mut buffer) {
                        Ok((size, source)) if source.ip().is_loopback() => {
                            if session.accept(&buffer[..size], &token, &mut previous) {
                                last_seen = Instant::now();
                            }
                        }
                        Ok(_) => {}
                        Err(error)
                            if matches!(
                                error.kind(),
                                std::io::ErrorKind::WouldBlock
                                    | std::io::ErrorKind::TimedOut
                                    | std::io::ErrorKind::Interrupted
                            ) => {}
                        Err(error) if error.raw_os_error() == Some(10040) => {}
                        Err(_) => break,
                    }
                    if last_seen.elapsed() >= Duration::from_secs(5) {
                        session.clear();
                    }
                }
                session.retire();
            });
    }

    fn is_retired(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .retired
    }

    fn accept(&self, data: &[u8], token: &str, previous: &mut u64) -> bool {
        if data.len() > MAX_PROGRESS_DATAGRAM_BYTES {
            return false;
        }
        let Ok(event) = serde_json::from_slice::<ProgressEvent>(data) else {
            return false;
        };
        if event.validate(token, *previous).is_err() {
            return false;
        }
        if !self.apply(&event.entries) {
            return false;
        }
        *previous = event.event_id;
        true
    }

    fn clear(&self) {
        let old = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.highest_id = 0;
            std::mem::take(&mut state.entries)
        };
        drop(old);
    }

    pub(super) fn apply(&self, entries: &[ProgressEntry]) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.retired {
            return false;
        }
        if entries
            .iter()
            .any(|entry| match state.entries.get(&entry.operation_id) {
                Some(old) => {
                    old.key != entry.key
                        || entry.revision < old.revision
                        || (entry.revision == old.revision && entry.percent != old.percent)
                }
                None => entry.operation_id <= state.highest_id,
            })
        {
            return false;
        }
        if let Some(highest) = entries.iter().map(|entry| entry.operation_id).max() {
            state.highest_id = state.highest_id.max(highest);
        }
        let ids: HashSet<u64> = entries.iter().map(|entry| entry.operation_id).collect();
        state.entries.retain(|id, _| ids.contains(id));
        let mut ordered: Vec<_> = entries.iter().collect();
        ordered.sort_unstable_by_key(|entry| entry.revision);
        for entry in ordered {
            let remote = state
                .entries
                .entry(entry.operation_id)
                .or_insert_with(|| RemoteEntry {
                    guard: crate::install_progress::begin_external(&entry.key),
                    key: entry.key.clone(),
                    percent: None,
                    revision: 0,
                });
            if remote.percent != entry.percent || remote.revision != entry.revision {
                if let Some(percent) = entry.percent {
                    remote.guard.report(percent);
                } else {
                    remote.guard.clear_measurement();
                }
                remote.percent = entry.percent;
                remote.revision = entry.revision;
            }
        }
        true
    }

    pub(super) fn retire(&self) {
        let old = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.retired = true;
            std::mem::take(&mut state.entries)
        };
        drop(old);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packets_reject_spoof_reordering_and_malformed_data_and_recover_after_loss() {
        let session = ProgressSession::new();
        let token = "01".repeat(sgt_recorder_protocol::TOKEN_BYTES);
        let mut previous = 0;
        let frame = |event_id, token: &str, percent| {
            serde_json::to_vec(&ProgressEvent {
                protocol_version: sgt_recorder_protocol::PROGRESS_PROTOCOL_VERSION,
                token: token.into(),
                event_id,
                entries: vec![ProgressEntry {
                    operation_id: 1,
                    key: "test-packet-progress".into(),
                    percent,
                    revision: if percent.is_some() { event_id } else { 0 },
                }],
            })
            .unwrap()
        };
        assert!(!session.accept(b"{", &token, &mut previous));
        assert!(!session.accept(
            &vec![b'x'; MAX_PROGRESS_DATAGRAM_BYTES + 1],
            &token,
            &mut previous
        ));
        assert!(!session.accept(
            &frame(
                1,
                &"02".repeat(sgt_recorder_protocol::TOKEN_BYTES),
                Some(1.0)
            ),
            &token,
            &mut previous
        ));
        assert!(session.accept(&frame(1, &token, Some(20.0)), &token, &mut previous));
        assert!(session.accept(&frame(3, &token, Some(60.0)), &token, &mut previous));
        assert!(!session.accept(&frame(2, &token, Some(99.0)), &token, &mut previous));
        assert_eq!(
            crate::install_progress::snapshot("test-packet-progress"),
            Some(60.0)
        );
        session.clear();
        assert_eq!(
            crate::install_progress::snapshot("test-packet-progress"),
            None
        );
        assert!(session.accept(&frame(4, &token, Some(70.0)), &token, &mut previous));
        session.retire();
        session.accept(&frame(5, &token, Some(90.0)), &token, &mut previous);
        assert_eq!(
            crate::install_progress::snapshot("test-packet-progress"),
            None
        );
    }

    #[test]
    fn listener_binds_loopback_only() {
        let socket = ProgressSession::bind().unwrap();
        assert!(socket.local_addr().unwrap().ip().is_loopback());
    }

    #[test]
    fn repeated_snapshot_does_not_mask_new_local_progress_but_new_measurement_does() {
        let session = ProgressSession::new();
        let mut entry = ProgressEntry {
            operation_id: 1,
            key: "test-progress-priority".into(),
            percent: Some(30.0),
            revision: 1,
        };
        session.apply(&[entry.clone()]);
        let local = crate::install_progress::begin("test-progress-priority");
        local.report(80.0);
        session.apply(&[entry.clone()]);
        assert_eq!(
            crate::install_progress::snapshot("test-progress-priority"),
            Some(80.0)
        );
        entry.revision = 2;
        session.apply(&[entry]);
        assert_eq!(
            crate::install_progress::snapshot("test-progress-priority"),
            Some(30.0)
        );
        session.retire();
        drop(local);
    }

    #[test]
    fn finishing_newer_worker_owner_does_not_promote_older_worker_measurement() {
        let session = ProgressSession::new();
        let first = ProgressEntry {
            operation_id: 1,
            key: "test-owner-order".into(),
            percent: Some(20.0),
            revision: 1,
        };
        let second = ProgressEntry {
            operation_id: 2,
            key: "test-owner-order".into(),
            percent: Some(30.0),
            revision: 2,
        };
        session.apply(&[first.clone(), second]);
        let local = crate::install_progress::begin("test-owner-order");
        local.report(80.0);
        session.apply(&[first]);
        assert_eq!(
            crate::install_progress::snapshot("test-owner-order"),
            Some(80.0)
        );
        session.retire();
        drop(local);
    }

    #[test]
    fn retired_operation_id_cannot_reappear_with_a_new_key() {
        let session = ProgressSession::new();
        let first = ProgressEntry {
            operation_id: 1,
            key: "test-retired-progress".into(),
            percent: Some(25.0),
            revision: 1,
        };
        assert!(session.apply(&[first]));
        assert!(session.apply(&[]));
        let stale = ProgressEntry {
            operation_id: 1,
            key: "test-retired-progress".into(),
            percent: Some(99.0),
            revision: 2,
        };
        assert!(!session.apply(&[stale]));
        assert_eq!(
            crate::install_progress::snapshot("test-retired-progress"),
            None
        );
        session.retire();
    }

    #[test]
    fn initial_snapshot_applies_measurements_in_revision_order() {
        let session = ProgressSession::new();
        let newer = ProgressEntry {
            operation_id: 1,
            key: "test-revision-order".into(),
            percent: Some(60.0),
            revision: 2,
        };
        let older = ProgressEntry {
            operation_id: 2,
            key: "test-revision-order".into(),
            percent: Some(40.0),
            revision: 1,
        };
        assert!(session.apply(&[newer, older]));
        assert_eq!(
            crate::install_progress::snapshot("test-revision-order"),
            Some(60.0)
        );
        session.retire();
    }

    #[test]
    fn loopback_snapshot_updates_and_disconnect_retires_owner() {
        let session = ProgressSession::new();
        let socket = ProgressSession::bind().unwrap();
        let address = socket.local_addr().unwrap();
        let token = "01".repeat(sgt_recorder_protocol::TOKEN_BYTES);
        session.start(socket, token.clone());
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        let event = ProgressEvent {
            protocol_version: sgt_recorder_protocol::PROGRESS_PROTOCOL_VERSION,
            token,
            event_id: 1,
            entries: vec![ProgressEntry {
                operation_id: 1,
                key: "test-loopback-progress".into(),
                percent: Some(35.0),
                revision: 1,
            }],
        };
        sender
            .send_to(&serde_json::to_vec(&event).unwrap(), address)
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while crate::install_progress::snapshot("test-loopback-progress") != Some(35.0)
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            crate::install_progress::snapshot("test-loopback-progress"),
            Some(35.0)
        );
        session.retire();
        assert_eq!(
            crate::install_progress::snapshot("test-loopback-progress"),
            None
        );
    }

    #[test]
    fn snapshots_replace_only_their_session_and_retirement_rejects_late_updates() {
        let old = ProgressSession::new();
        let new = ProgressSession::new();
        let entry = |key: &str, percent| ProgressEntry {
            operation_id: 1,
            key: key.into(),
            percent,
            revision: if percent.is_some() { 1 } else { 0 },
        };
        old.apply(&[entry("test-recorder-progress", Some(20.0))]);
        new.apply(&[entry("test-recorder-progress", Some(50.0))]);
        assert_eq!(
            crate::install_progress::snapshot("test-recorder-progress"),
            Some(50.0)
        );
        old.retire();
        old.apply(&[entry("test-recorder-progress", Some(99.0))]);
        assert_eq!(
            crate::install_progress::snapshot("test-recorder-progress"),
            Some(50.0)
        );
        let mut replacement = entry("test-recorder-progress", None);
        replacement.operation_id = 2;
        new.apply(&[replacement]);
        assert_eq!(
            crate::install_progress::snapshot("test-recorder-progress"),
            Some(-1.0)
        );
        new.retire();
        assert_eq!(
            crate::install_progress::snapshot("test-recorder-progress"),
            None
        );
    }
}
