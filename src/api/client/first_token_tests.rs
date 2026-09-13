use super::*;
use std::io::{BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

fn server(
    response: impl FnOnce(&mut TcpStream) + Send + 'static,
) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/stream", listener.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut headers = Vec::new();
        while !headers.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            socket.read_exact(&mut byte).unwrap();
            headers.push(byte[0]);
        }
        response(&mut socket);
    });
    (url, worker)
}

fn headers(socket: &mut TcpStream) {
    socket
        .write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
        )
        .unwrap();
}

fn token(socket: &mut TcpStream) -> std::io::Result<()> {
    socket.write_all(b"data: {\"choices\":[{\"delta\":{\"content\":\"x\"}}]}\n\n")
}

fn read(
    url: &str,
    budget: Duration,
    cancelled: Option<Arc<AtomicBool>>,
    on_chunk: impl FnMut(&str),
) -> anyhow::Result<String> {
    let timeouts = super::super::RequestTimeouts::uniform(budget);
    let _guard = FirstTokenGuard::new(true, Some(timeouts), &cancelled);
    let response = super::super::with_request_timeouts(
        super::super::UREQ_STREAM_RESPONSE_AGENT.get(url),
        Some(timeouts),
    )
    .call()?;
    crate::api::openai_compat::consume_content_stream(
        BufReader::new(response.into_body().into_reader()),
        &cancelled,
        &mut { on_chunk },
    )
}

#[test]
fn output_continues_past_all_initial_budgets_and_long_gaps() {
    let (url, worker) = server(|socket| {
        headers(socket);
        token(socket).unwrap();
        std::thread::sleep(Duration::from_millis(450));
        for _ in 0..5 {
            token(socket).unwrap();
            std::thread::sleep(Duration::from_millis(90));
        }
        socket.write_all(b"data: [DONE]\n\n").unwrap();
    });
    let result = read(&url, Duration::from_millis(250), None, |_| {});
    worker.join().unwrap();
    assert_eq!(result.unwrap(), "xxxxxx");
}

#[test]
fn headers_and_continuous_reasoning_do_not_satisfy_first_token() {
    let (url, worker) = server(|socket| {
        headers(socket);
        for _ in 0..20 {
            if socket.write_all(b": keepalive\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"\",\"reasoning\":\"thinking\"}}]}\n\n").is_err() { break; }
            std::thread::sleep(Duration::from_millis(35));
        }
    });
    let started = Instant::now();
    let result = read(&url, Duration::from_millis(200), None, |_| {
        panic!("no content")
    });
    let elapsed = started.elapsed();
    worker.join().unwrap();
    assert!(result.unwrap_err().to_string().contains("timeout"));
    assert!(elapsed < Duration::from_millis(650));
}

#[test]
fn missing_headers_remain_bounded() {
    let (url, worker) = server(|_| std::thread::sleep(Duration::from_millis(500)));
    let started = Instant::now();
    let result = read(&url, Duration::from_millis(150), None, |_| {});
    let elapsed = started.elapsed();
    worker.join().unwrap();
    assert!(result.unwrap_err().to_string().contains("timeout"));
    assert!(elapsed < Duration::from_millis(450));
}

#[test]
fn cancellation_remains_responsive_after_first_output() {
    let (url, worker) = server(|socket| {
        headers(socket);
        token(socket).unwrap();
        std::thread::sleep(Duration::from_millis(500));
    });
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = cancelled.clone();
    let started = Instant::now();
    let result = read(
        &url,
        Duration::from_millis(250),
        Some(cancelled),
        move |_| {
            flag.store(true, Ordering::Relaxed);
        },
    );
    let elapsed = started.elapsed();
    worker.join().unwrap();
    assert!(result.unwrap_err().to_string().contains("Cancelled"));
    assert!(elapsed < Duration::from_millis(450));
}

#[test]
fn completed_scope_does_not_disable_the_next_attempt_deadline() {
    {
        let _guard = FirstTokenGuard::new(
            true,
            Some(super::super::RequestTimeouts::uniform(
                Duration::from_millis(1),
            )),
            &None,
        );
        received();
        assert!(read_timeout().unwrap().is_some());
    }
    assert!(!active());
    let _guard = FirstTokenGuard::new(
        true,
        Some(super::super::RequestTimeouts::uniform(
            Duration::from_millis(1),
        )),
        &None,
    );
    read_timeout().unwrap();
    std::thread::sleep(Duration::from_millis(5));
    assert!(read_timeout().is_err());
}

#[test]
fn silent_stream_expires_after_output_has_started() {
    let _guard = FirstTokenGuard::new(true, None, &None);
    received();
    CURRENT.with(|current| {
        current.borrow_mut().as_mut().unwrap().started =
            Some(Instant::now() - Duration::from_secs(600));
    });
    assert!(
        read_timeout().is_err(),
        "output must not disable idle protection"
    );
}

#[test]
fn idle_allowance_is_generous_and_respects_longer_callers() {
    for (first, total, expected) in [(1, 5, 120), (300, 5, 300), (1, 600, 600)] {
        let mut timeouts = super::super::RequestTimeouts::uniform(Duration::from_secs(first));
        timeouts.total = Duration::from_secs(total);
        let _guard = FirstTokenGuard::new(true, Some(timeouts), &None);
        CURRENT.with(|current| {
            let current = current.borrow();
            let deadline = current.as_ref().unwrap();
            assert_eq!(deadline.budget, Duration::from_secs(first));
            assert_eq!(deadline.idle_budget, Duration::from_secs(expected));
        });
    }
}

#[test]
fn every_output_renews_idle_but_retry_restores_first_output_budget() {
    let _guard = FirstTokenGuard::new(true, None, &None);
    for _ in 0..3 {
        CURRENT.with(|current| {
            current.borrow_mut().as_mut().unwrap().started =
                Some(Instant::now() - Duration::from_secs(600));
        });
        received();
        assert!(read_timeout().is_ok());
    }
    begin_attempt();
    CURRENT.with(|current| {
        let current = current.borrow();
        let deadline = current.as_ref().unwrap();
        assert!(!deadline.received);
        assert!(deadline.started.is_none());
    });
}

#[test]
fn heartbeat_only_stream_times_out_after_real_output() {
    let (url, worker) = server(|socket| {
        headers(socket);
        token(socket).unwrap();
        for _ in 0..30 {
            if socket
                .write_all(
                    b": keepalive\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"\"}}]}\n\n",
                )
                .is_err()
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
    });
    let started = Instant::now();
    let result = read(&url, Duration::from_secs(2), None, |_| {
        // Scale only the test's post-output allowance; production retains the
        // generous floor independently of the first-output budget.
        CURRENT.with(|current| {
            current.borrow_mut().as_mut().unwrap().idle_budget = Duration::from_millis(200);
        });
    });
    let elapsed = started.elapsed();
    worker.join().unwrap();
    assert!(result.unwrap_err().to_string().contains("timeout"));
    assert!(elapsed < Duration::from_millis(750));
}

#[test]
fn quiet_open_connection_times_out_without_waiting_for_eof() {
    let (url, worker) = server(|socket| {
        headers(socket);
        token(socket).unwrap();
        std::thread::sleep(Duration::from_millis(800));
    });
    let started = Instant::now();
    let result = read(&url, Duration::from_secs(2), None, |_| {
        CURRENT.with(|current| {
            current.borrow_mut().as_mut().unwrap().idle_budget = Duration::from_millis(200);
        });
    });
    let elapsed = started.elapsed();
    worker.join().unwrap();
    assert!(result.unwrap_err().to_string().contains("timeout"));
    assert!(elapsed < Duration::from_millis(650));
}

#[test]
fn active_stream_outlives_multiple_idle_windows() {
    let (url, worker) = server(|socket| {
        headers(socket);
        for _ in 0..8 {
            token(socket).unwrap();
            std::thread::sleep(Duration::from_millis(90));
        }
        socket.write_all(b"data: [DONE]\n\n").unwrap();
    });
    let result = read(&url, Duration::from_millis(500), None, |_| {
        CURRENT.with(|current| {
            current.borrow_mut().as_mut().unwrap().idle_budget = Duration::from_millis(300);
        });
    });
    worker.join().unwrap();
    assert_eq!(result.unwrap(), "xxxxxxxx");
}

#[test]
fn native_tls_survives_read_polls_after_first_output() {
    // This identity is public test data, trusted only by this loopback client.
    let cert = include_bytes!("fixtures/localhost-cert.pem");
    let key = include_bytes!("fixtures/localhost-test-key.pem");
    let identity = native_tls::Identity::from_pkcs8(cert, key).unwrap();
    let acceptor = native_tls::TlsAcceptor::new(identity).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "https://localhost:{}/",
        listener.local_addr().unwrap().port()
    );
    let worker = std::thread::spawn(move || {
        let (socket, _) = listener.accept().unwrap();
        let mut tls = acceptor.accept(socket).unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            tls.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        tls.write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"x\"}}]}\n\n").unwrap();
        std::thread::sleep(Duration::from_millis(450));
        tls.write_all(b"data: {\"choices\":[{\"delta\":{\"content\":\"y\"}}]}\n\ndata: [DONE]\n\n")
            .unwrap();
    });
    let tls = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::NativeTls)
        .root_certs(ureq::tls::RootCerts::Specific(std::sync::Arc::new(vec![
            ureq::tls::Certificate::from_pem(cert).unwrap(),
        ])))
        .build();
    let agent =
        super::super::token_transport::agent(ureq::Agent::config_builder().tls_config(tls).build());
    let mut budget = super::super::RequestTimeouts::uniform(Duration::from_millis(250));
    budget.connect = Duration::from_secs(3);
    budget.send = Duration::from_secs(3);
    let _guard = FirstTokenGuard::new(true, Some(budget), &None);
    let response = super::super::with_request_timeouts(agent.get(url), Some(budget))
        .call()
        .unwrap();
    let result = crate::api::openai_compat::consume_content_stream(
        BufReader::new(response.into_body().into_reader()),
        &None,
        &mut |_| {},
    );
    worker.join().unwrap();
    assert_eq!(result.unwrap(), "xy");
}

#[test]
fn reused_connection_gets_a_fresh_first_token_deadline() {
    let (url, worker) = server(|socket| {
        let body = b"data: {\"choices\":[{\"delta\":{\"content\":\"x\"}}]}\n\n";
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
            body.len()
        )
        .unwrap();
        socket.write_all(body).unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            socket.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        // A second request arrived on the same socket, without any output.
        std::thread::sleep(Duration::from_millis(450));
    });
    assert_eq!(
        read(&url, Duration::from_millis(150), None, |_| {}).unwrap(),
        "x"
    );
    let result = read(&url, Duration::from_millis(150), None, |_| {});
    worker.join().unwrap();
    assert!(result.unwrap_err().to_string().contains("timeout"));
}
