use parking_lot::Mutex;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::process::ChildStdout;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{Receiver, Sender, SyncSender};

const MAX_JSON_RPC_LINE_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ClientLifecycleKind {
    ToolsChanged,
    Disconnected,
}

const TOOLS_CHANGED_BIT: u8 = 1;
const DISCONNECTED_BIT: u8 = 2;

#[derive(Clone)]
pub(super) struct ClientLifecycleSignal {
    pending: Arc<AtomicU8>,
    wake: SyncSender<()>,
}

pub(super) struct ClientLifecycleEvents {
    connection_token: u64,
    pending: Arc<AtomicU8>,
    wake: Receiver<()>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ClientLifecycleBatch {
    pub tools_changed: bool,
    pub disconnected: bool,
}

pub(super) fn lifecycle_channel(
    connection_token: u64,
) -> (ClientLifecycleSignal, ClientLifecycleEvents) {
    let pending = Arc::new(AtomicU8::new(0));
    let (wake_tx, wake_rx) = std::sync::mpsc::sync_channel(1);
    (
        ClientLifecycleSignal {
            pending: Arc::clone(&pending),
            wake: wake_tx,
        },
        ClientLifecycleEvents {
            connection_token,
            pending,
            wake: wake_rx,
        },
    )
}

impl ClientLifecycleSignal {
    pub fn raise(&self, kind: ClientLifecycleKind) {
        let bit = match kind {
            ClientLifecycleKind::ToolsChanged => TOOLS_CHANGED_BIT,
            ClientLifecycleKind::Disconnected => DISCONNECTED_BIT,
        };
        self.pending.fetch_or(bit, Ordering::SeqCst);
        let _ = self.wake.try_send(());
    }
}

impl ClientLifecycleEvents {
    pub fn connection_token(&self) -> u64 {
        self.connection_token
    }

    pub fn recv(&self) -> Option<ClientLifecycleBatch> {
        loop {
            self.wake.recv().ok()?;
            let pending = self.pending.swap(0, Ordering::SeqCst);
            if pending != 0 {
                return Some(ClientLifecycleBatch {
                    tools_changed: pending & TOOLS_CHANGED_BIT != 0,
                    disconnected: pending & DISCONNECTED_BIT != 0,
                });
            }
        }
    }
}

pub(super) fn reader_loop(
    stdout: ChildStdout,
    pending: &Mutex<HashMap<u64, Sender<Value>>>,
    alive: &AtomicBool,
    lifecycle: Option<&ClientLifecycleSignal>,
) {
    read_messages(BufReader::new(stdout), pending, alive, lifecycle);
}

fn read_messages(
    mut reader: impl BufRead,
    pending: &Mutex<HashMap<u64, Sender<Value>>>,
    alive: &AtomicBool,
    lifecycle: Option<&ClientLifecycleSignal>,
) {
    let mut line = String::new();
    loop {
        line.clear();
        match read_protocol_line(&mut reader, &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => handle_message(&line, pending, lifecycle),
        }
    }
    alive.store(false, Ordering::SeqCst);
    pending.lock().clear();
    raise(lifecycle, ClientLifecycleKind::Disconnected);
}

fn handle_message(
    line: &str,
    pending: &Mutex<HashMap<u64, Sender<Value>>>,
    lifecycle: Option<&ClientLifecycleSignal>,
) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }
    let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
        return;
    };
    if let Some(id) = value.get("id").and_then(Value::as_u64) {
        if let Some(tx) = pending.lock().remove(&id) {
            let _ = tx.send(value);
        }
        return;
    }
    if value.get("method").and_then(Value::as_str) == Some("notifications/tools/list_changed") {
        raise(lifecycle, ClientLifecycleKind::ToolsChanged);
    }
}

fn raise(lifecycle: Option<&ClientLifecycleSignal>, kind: ClientLifecycleKind) {
    if let Some(lifecycle) = lifecycle {
        lifecycle.raise(kind);
    }
}

fn read_protocol_line(reader: &mut impl BufRead, line: &mut String) -> std::io::Result<usize> {
    let bytes = reader
        .by_ref()
        .take(MAX_JSON_RPC_LINE_BYTES + 1)
        .read_line(line)?;
    if bytes as u64 > MAX_JSON_RPC_LINE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "MCP JSON-RPC line exceeds the bounded transport limit",
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::mpsc;

    fn read_fixture(input: Vec<u8>) -> Vec<ClientLifecycleBatch> {
        let pending = Mutex::new(HashMap::new());
        let alive = AtomicBool::new(true);
        let (signal, events) = lifecycle_channel(41);
        read_messages(Cursor::new(input), &pending, &alive, Some(&signal));
        assert!(!alive.load(Ordering::SeqCst));
        drop(signal);
        std::iter::from_fn(|| events.recv()).collect()
    }

    #[test]
    fn recognizes_idless_tool_change_without_capability_state() {
        let events = read_fixture(
            b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/tools/list_changed\"}\n".to_vec(),
        );

        assert_eq!(
            events,
            [ClientLifecycleBatch {
                tools_changed: true,
                disconnected: true,
            }]
        );
    }

    #[test]
    fn routes_responses_and_ignores_unrelated_notifications() {
        let (response_tx, response_rx) = mpsc::channel();
        let pending = Mutex::new(HashMap::from([(7, response_tx)]));
        let alive = AtomicBool::new(true);
        let (signal, events) = lifecycle_channel(9);
        let input = concat!(
            "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\"}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":7,\"result\":{}}\n"
        );

        read_messages(
            Cursor::new(input.as_bytes()),
            &pending,
            &alive,
            Some(&signal),
        );
        drop(signal);

        assert_eq!(response_rx.recv().unwrap()["id"], 7);
        assert_eq!(
            std::iter::from_fn(|| events.recv()).collect::<Vec<_>>(),
            [ClientLifecycleBatch {
                tools_changed: false,
                disconnected: true,
            }]
        );
    }

    #[test]
    fn eof_emits_disconnect_and_wakes_pending_requests() {
        let (response_tx, response_rx) = mpsc::channel();
        let pending = Mutex::new(HashMap::from([(1, response_tx)]));
        let events = {
            let alive = AtomicBool::new(true);
            let (signal, events) = lifecycle_channel(3);
            read_messages(
                Cursor::new(Vec::<u8>::new()),
                &pending,
                &alive,
                Some(&signal),
            );
            drop(signal);
            std::iter::from_fn(|| events.recv()).collect::<Vec<_>>()
        };

        assert!(response_rx.recv().is_err());
        assert!(events[0].disconnected);
    }

    #[test]
    fn oversized_line_disconnects_without_unbounded_buffering() {
        let mut input = vec![b'x'; MAX_JSON_RPC_LINE_BYTES as usize + 1];
        input.push(b'\n');
        let events = read_fixture(input);

        assert_eq!(events.len(), 1);
        assert!(events[0].disconnected);
    }

    #[test]
    fn burst_is_bounded_nonblocking_and_preserves_a_later_dirty_edge() {
        let (signal, events) = lifecycle_channel(71);
        for _ in 0..100_000 {
            signal.raise(ClientLifecycleKind::ToolsChanged);
        }
        assert_eq!(events.connection_token(), 71);
        assert_eq!(
            events.recv(),
            Some(ClientLifecycleBatch {
                tools_changed: true,
                disconnected: false,
            })
        );

        signal.raise(ClientLifecycleKind::ToolsChanged);
        assert!(events.recv().is_some_and(|batch| batch.tools_changed));
    }
}
