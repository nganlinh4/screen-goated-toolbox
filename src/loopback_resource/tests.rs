use super::*;
use std::sync::atomic::AtomicUsize;

fn socket_pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    (listener.accept().unwrap().0, client)
}

fn connection(stream: TcpStream, timeout: Duration) -> Connection {
    Connection {
        stream,
        deadline: Instant::now() + timeout,
        stop: Arc::new(AtomicBool::new(false)),
    }
}

#[test]
fn partial_read_progress_cannot_renew_the_deadline() {
    let (server, mut client) = socket_pair();
    let sender = thread::spawn(move || {
        for _ in 0..20 {
            if client.write_all(b"x").is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
    });
    let mut reader = connection(server, Duration::from_millis(80));
    let started = Instant::now();
    let error = reader.read_exact(&mut [0_u8; 20]).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(1));
    drop(reader);
    sender.join().unwrap();
}

#[test]
fn successful_writes_cannot_renew_the_deadline() {
    let (server, _client) = socket_pair();
    let mut writer = connection(server, Duration::from_millis(80));
    let started = Instant::now();
    let mut failure = None;
    for _ in 0..20 {
        if let Err(error) = writer.write_all(b"x") {
            failure = Some(error);
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(failure.unwrap().kind(), io::ErrorKind::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn expired_queued_connection_is_not_given_a_fresh_io_budget() {
    let (server, mut client) = socket_pair();
    client.write_all(b"ready").unwrap();
    let mut reader = Connection {
        stream: server,
        deadline: Instant::now() - Duration::from_millis(1),
        stop: Arc::new(AtomicBool::new(false)),
    };
    assert_eq!(
        reader.read(&mut [0_u8; 5]).unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
}

#[test]
fn bounded_queue_rejects_excess_sockets_without_waiting_for_a_worker() {
    let pairs: Vec<_> = (0..3).map(|_| socket_pair()).collect();
    let (servers, mut clients): (Vec<_>, Vec<_>) = pairs.into_iter().unzip();
    let (sender, receiver) = mpsc::sync_channel(1);
    let stop = Arc::new(AtomicBool::new(false));
    let accept_stop = stop.clone();
    let (done_sender, done_receiver) = mpsc::channel();
    let worker = thread::spawn(move || {
        let mut servers = servers.into_iter();
        accept_loop(
            || match servers.next() {
                Some(stream) => Ok(stream),
                None => {
                    accept_stop.store(true, Ordering::Release);
                    Err(io::ErrorKind::Interrupted.into())
                }
            },
            sender,
            &accept_stop,
            Duration::from_secs(1),
        );
        done_sender.send(()).unwrap();
    });
    done_receiver.recv_timeout(Duration::from_secs(2)).unwrap();
    worker.join().unwrap();
    let queued = receiver.try_recv().unwrap();
    assert!(queued.deadline <= Instant::now() + Duration::from_secs(1));
    assert!(receiver.try_recv().is_err());
    for client in &mut clients[1..] {
        client
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        assert!(matches!(client.read(&mut [0_u8; 1]), Ok(0)));
    }
}

#[test]
fn repeated_accept_errors_have_a_finite_retry_budget() {
    let (sender, _receiver) = mpsc::sync_channel(1);
    let stop = AtomicBool::new(false);
    let mut attempts = 0;
    accept_loop(
        || {
            attempts += 1;
            Err(io::ErrorKind::Other.into())
        },
        sender,
        &stop,
        Duration::from_secs(1),
    );
    assert_eq!(attempts, MAX_ACCEPT_ERRORS);
    assert!(stop.load(Ordering::Acquire));
}

#[test]
fn worker_count_and_shutdown_stay_bounded_with_idle_clients() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let (started_sender, started_receiver) = mpsc::channel();
    let worker_active = active.clone();
    let worker_peak = peak.clone();
    let server = Server::start(
        listener,
        Limits {
            workers: 2,
            queue: 2,
            timeout: Duration::from_secs(5),
        },
        move |connection| {
            let count = worker_active.fetch_add(1, Ordering::SeqCst) + 1;
            worker_peak.fetch_max(count, Ordering::SeqCst);
            started_sender.send(()).unwrap();
            let _ = connection.read_exact(&mut [0_u8; 1]);
            worker_active.fetch_sub(1, Ordering::SeqCst);
        },
    )
    .unwrap();
    let _clients: Vec<_> = (0..8)
        .map(|_| TcpStream::connect(address).unwrap())
        .collect();
    for _ in 0..2 {
        started_receiver
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
    }
    assert_eq!(peak.load(Ordering::SeqCst), 2);
    let started = Instant::now();
    drop(server);
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(active.load(Ordering::SeqCst), 0);
}

#[test]
fn idle_listener_and_receivers_stop_with_the_owner() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = Server::start(listener, Limits::default(), |_| {
        panic!("No request was admitted")
    })
    .unwrap();
    drop(server);
    assert!(TcpStream::connect(address).is_err());
}

#[test]
fn invalid_limits_fail_before_starting_a_pool() {
    for limits in [
        Limits {
            workers: 0,
            ..Limits::default()
        },
        Limits {
            queue: 0,
            ..Limits::default()
        },
        Limits {
            timeout: Duration::ZERO,
            ..Limits::default()
        },
        Limits {
            timeout: Duration::MAX,
            ..Limits::default()
        },
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        assert!(Server::start(listener, limits, |_| {}).is_err());
    }
}
