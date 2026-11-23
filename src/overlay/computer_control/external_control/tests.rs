use std::io::Read;

use super::*;

#[test]
fn oversized_content_length_is_rejected_without_stopping_listener() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        for _ in 0..2 {
            let (stream, _) = listener.accept().unwrap();
            handle_connection(stream, "test-token").unwrap();
        }
    });
    for (request, expected) in [
        (
            format!(
                "POST /v1/control HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
                usize::MAX
            ),
            "HTTP/1.1 413",
        ),
        (
            "GET /v1/status HTTP/1.1\r\n\r\n".to_string(),
            "HTTP/1.1 401",
        ),
    ] {
        let mut client = TcpStream::connect(address).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        client.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        assert!(response.starts_with(expected), "{response}");
    }
    worker.join().unwrap();
}

#[test]
fn routing_rejects_unknown_and_untyped_requests() {
    let request = Request {
        method: "POST".into(),
        path: "/v1/control".into(),
        authorization: None,
        content_type: Some("application/json".into()),
        body: br#"{"operation":"future"}"#.to_vec(),
    };
    assert_eq!(route(&request).status, 400);
    let mut wrong_type = request;
    wrong_type.content_type = Some("text/plain".into());
    assert_eq!(route(&wrong_type).status, 415);
}

fn assert_trickle_times_out(prefix: &'static [u8]) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (finished, result) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let deadline = Instant::now() + Duration::from_millis(100);
        let error = read_request(&mut stream, deadline).err().unwrap();
        finished
            .send(error.downcast_ref::<ReadError>() == Some(&ReadError::Timeout))
            .unwrap();
    });
    let mut stream = TcpStream::connect(address).unwrap();
    stream.write_all(prefix).unwrap();
    let writer = std::thread::spawn(move || {
        for _ in 0..100 {
            std::thread::sleep(Duration::from_millis(20));
            if stream.write_all(b"x").is_err() {
                break;
            }
        }
    });
    assert!(result.recv_timeout(Duration::from_secs(1)).unwrap());
    worker.join().unwrap();
    writer.join().unwrap();
}

#[test]
fn header_trickle_cannot_extend_the_request_deadline() {
    assert_trickle_times_out(b"G");
}

#[test]
fn body_trickle_cannot_extend_the_request_deadline() {
    assert_trickle_times_out(b"POST /v1/control HTTP/1.1\r\nContent-Length: 4000\r\n\r\n");
}

#[test]
fn expired_request_is_rejected_even_when_complete_bytes_are_ready() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    client
        .write_all(b"GET /v1/status HTTP/1.1\r\n\r\n")
        .unwrap();
    let (mut stream, _) = listener.accept().unwrap();
    let error = read_request(&mut stream, Instant::now()).err().unwrap();
    assert_eq!(error.downcast_ref::<ReadError>(), Some(&ReadError::Timeout));
}

#[test]
fn silent_connection_expires_then_listener_serves_the_next_authenticated_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let worker = std::thread::spawn(move || serve(listener, "test-token", &worker_stop));
    let mut silent = TcpStream::connect(address).unwrap();
    silent
        .set_read_timeout(Some(Duration::from_secs(4)))
        .unwrap();
    let mut timeout_response = String::new();
    silent.read_to_string(&mut timeout_response).unwrap();
    let mut client = TcpStream::connect(address).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    client
        .write_all(b"GET /not-found HTTP/1.1\r\nAuthorization: Bearer test-token\r\n\r\n")
        .unwrap();
    let mut response = String::new();
    client.read_to_string(&mut response).unwrap();
    stop.store(true, Ordering::SeqCst);
    let _ = TcpStream::connect(address);
    worker.join().unwrap();
    assert!(
        timeout_response.starts_with("HTTP/1.1 408"),
        "{timeout_response}"
    );
    assert!(timeout_response.contains("request_timeout"));
    assert!(response.starts_with("HTTP/1.1 404"), "{response}");
}

#[test]
fn stopped_listener_does_not_read_the_shutdown_wake_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let (done, completion) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        serve(listener, "test-token", &worker_stop);
        done.send(()).unwrap();
    });
    let mut client = TcpStream::connect(address).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    client
        .write_all(b"GET /v1/status HTTP/1.1\r\n\r\n")
        .unwrap();
    let mut response = String::new();
    client.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 401"));
    stop.store(true, Ordering::SeqCst);
    let _ = TcpStream::connect(address);
    completion.recv_timeout(Duration::from_secs(1)).unwrap();
    worker.join().unwrap();
}

#[test]
fn launch_and_submit_during_shutdown_report_stopping_instead_of_success() {
    let control = |body: &str| Request {
        method: "POST".into(),
        path: "/v1/control".into(),
        authorization: None,
        content_type: Some("application/json".into()),
        body: body.as_bytes().to_vec(),
    };
    let (launch, submit) = super::super::overlay::with_stopping_session(|| {
        (
            route(&control(r#"{"operation":"launch"}"#)),
            route(&control(
                r#"{"operation":"submit_turn","prompt":"open notepad"}"#,
            )),
        )
    });
    for answer in [launch, submit] {
        assert_eq!(answer.status, 409);
        assert_eq!(answer.body["code"], "stopping");
    }
    assert!(
        !text_command_queued(),
        "a refused command must not stay queued"
    );
    let credentials = unavailable(SessionUnavailable::Credentials);
    assert_eq!(
        (credentials.status, credentials.body["code"].as_str()),
        (409, Some("credentials_required"))
    );
}
