use super::*;
use std::cell::Cell;
use std::net::{TcpListener, TcpStream};
use tungstenite::Message;

#[test]
fn shared_transport_cases_require_complete_nonempty_audio() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../parity-fixtures/tts-runtime/edge-transport.json"
    ))
    .unwrap();
    assert_eq!(fixture["limits"]["setup_seconds"], SETUP_BUDGET.as_secs());
    assert_eq!(
        fixture["limits"]["session_seconds"],
        SESSION_BUDGET.as_secs()
    );
    assert_eq!(fixture["limits"]["encoded_audio_bytes"], MAX_AUDIO_BYTES);
    for case in fixture["cases"].as_array().unwrap() {
        let events = case["events"].as_array().unwrap();
        let index = Cell::new(0);
        let event = || {
            events
                .get(index.get())
                .and_then(|value| value.as_str())
                .unwrap_or("error")
        };
        let result = receive_audio(
            || {
                let current = event();
                index.set(index.get() + 1);
                match current {
                    "audio" | "audio_overflow" => {
                        let header = b"Path:audio\r\n";
                        let mut frame = (header.len() as u16).to_be_bytes().to_vec();
                        frame.extend_from_slice(header);
                        frame.resize(
                            frame.len()
                                + if current == "audio" {
                                    1
                                } else {
                                    MAX_AUDIO_BYTES + 1
                                },
                            1,
                        );
                        Ok(Message::Binary(frame.into()))
                    }
                    "turn_end" => Ok(Message::Text("Path:turn.end\r\n\r\n".into())),
                    "unterminated_turn" => Ok(Message::Text("Path:turn.end".into())),
                    "malformed_audio" => Ok(Message::Binary(vec![0, 8, 1].into())),
                    "close" => Ok(Message::Close(None)),
                    _ => Err(tungstenite::Error::ConnectionClosed),
                }
            },
            || event() == "cancel",
            || matches!(event(), "session_timeout" | "setup_timeout"),
        );
        let actual = match result {
            Ok(_) => "complete",
            Err("cancelled") => "cancelled",
            Err(_) => "failure",
        };
        assert_eq!(
            actual,
            case["expected"].as_str().unwrap(),
            "case {}",
            case["name"]
        );
    }
}

#[test]
fn completion_marker_must_be_an_exact_header() {
    assert!(is_turn_end("Path:turn.end\r\n\r\n"));
    assert!(!is_turn_end("Path:other\r\n\r\nPath:turn.end"));
    assert!(!is_turn_end("Path:turn.end-extra\r\n\r\n"));
    assert!(!is_turn_end("Path:turn.end"));
}

#[test]
fn nonblocking_socket_honors_cancellation_and_session_deadline() {
    for cancelled_case in [true, false] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (_peer, _) = listener.accept().unwrap();
        client.set_nonblocking(true).unwrap();
        let mut socket = tungstenite::WebSocket::from_raw_socket(
            client,
            tungstenite::protocol::Role::Client,
            None,
        );
        let started = Instant::now();
        let result = receive_audio(
            || socket.read(),
            || cancelled_case && started.elapsed() >= Duration::from_millis(60),
            || !cancelled_case && started.elapsed() >= Duration::from_millis(60),
        );
        assert_eq!(
            result.unwrap_err(),
            if cancelled_case {
                "cancelled"
            } else {
                "Edge synthesis timed out"
            }
        );
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}

#[test]
fn setup_wait_checks_cancellation_and_absolute_deadline() {
    assert_eq!(
        wait_for_io(Instant::now(), Duration::from_secs(1), &|| true),
        Err("cancelled")
    );
    assert_eq!(
        wait_for_io(Instant::now(), Duration::ZERO, &|| false),
        Err("Edge connection timed out")
    );
}

#[test]
fn nonblocking_write_resumes_without_duplicating_message() {
    use std::io::{self, Read, Write};
    struct Backpressure {
        blocked: bool,
        bytes: Vec<u8>,
    }
    impl Read for Backpressure {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::ErrorKind::WouldBlock.into())
        }
    }
    impl Write for Backpressure {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            if !self.blocked {
                self.blocked = true;
                return Err(io::ErrorKind::WouldBlock.into());
            }
            self.bytes.extend_from_slice(data);
            Ok(data.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let io = Backpressure {
        blocked: false,
        bytes: Vec::new(),
    };
    let mut sender =
        tungstenite::WebSocket::from_raw_socket(io, tungstenite::protocol::Role::Client, None);
    send_message(
        &mut sender,
        Message::Text("once".into()),
        Instant::now(),
        &|| false,
    )
    .unwrap();
    let bytes = sender.into_inner().bytes;
    let mut receiver = tungstenite::WebSocket::from_raw_socket(
        std::io::Cursor::new(bytes),
        tungstenite::protocol::Role::Server,
        None,
    );
    assert_eq!(receiver.read().unwrap(), Message::Text("once".into()));
    assert!(receiver.read().is_err());
}
