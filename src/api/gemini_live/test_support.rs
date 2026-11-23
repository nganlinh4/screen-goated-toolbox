use std::io::{self, Read};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use super::{GeminiLiveManager, LiveInputContent, LiveRequest, LiveResponse, wait::WaitOptions};

pub(super) fn request(
    manager: &GeminiLiveManager,
    cancel: Option<Arc<AtomicBool>>,
) -> (LiveResponse, WaitOptions) {
    let first = Arc::new(AtomicBool::new(false));
    let options = WaitOptions {
        cancel_token: cancel.clone(),
        deadline: None,
        request_timeout: None,
        first_output_received: first.clone(),
    };
    let response = manager.request(LiveRequest {
        api_key: "fixture-key".to_string(),
        model: "fixture-model".to_string(),
        content: LiveInputContent::Text("request".to_string()),
        instruction: String::new(),
        show_thinking: false,
        cancel_token: cancel,
        deadline: None,
        first_output_received: first,
    });
    (response, options)
}

pub(super) fn tcp_pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let peer = listener.accept().unwrap().0;
    for socket in [&client, &peer] {
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
    }
    (client, peer)
}

pub(super) fn assert_closed(mut peer: TcpStream) {
    match peer.read(&mut [0u8; 1]) {
        Ok(0) => {}
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::ConnectionReset
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::BrokenPipe
                    | io::ErrorKind::NotConnected
            ) => {}
        other => panic!("owned transport was not closed: {other:?}"),
    }
}

pub(super) fn publish(manager: &GeminiLiveManager, event: super::LiveEvent) {
    assert!(
        manager
            .work_queue
            .lock()
            .unwrap()
            .front()
            .unwrap()
            .publish(event)
    );
}

pub(super) fn cancel_flag() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

pub(super) fn assert_not_cancelled(flag: &AtomicBool) {
    assert!(!flag.load(Ordering::SeqCst));
}
