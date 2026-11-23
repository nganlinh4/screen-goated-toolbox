use super::*;
use std::sync::{Mutex, mpsc};

static TEST_RESOLVERS: AtomicUsize = AtomicUsize::new(0);
static DNS_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn cancelled_dns_wait_retains_resolver_capacity_until_worker_settles() {
    let _lock = DNS_TEST_LOCK.lock().unwrap();
    let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let observed = cancelled.clone();
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let thread = std::thread::spawn(move || {
        resolve_with_cancel(
            &TEST_RESOLVERS,
            Duration::from_secs(2),
            &|| observed.load(Ordering::SeqCst),
            move || {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(Vec::new())
            },
        )
    });
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    cancelled.store(true, Ordering::SeqCst);
    let result = thread.join().unwrap().unwrap_err();
    assert_eq!(result.kind(), io::ErrorKind::Interrupted);
    assert_eq!(TEST_RESOLVERS.load(Ordering::SeqCst), 1);
    release_tx.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while TEST_RESOLVERS.load(Ordering::SeqCst) != 0 {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}

#[test]
fn cancelled_connect_drops_a_late_success_and_skips_fallback() {
    let cancelled = std::cell::Cell::new(false);
    let addresses = vec![
        "192.0.2.1:443".parse().unwrap(),
        "192.0.2.2:443".parse().unwrap(),
    ];
    let mut attempts = 0;
    let result = connect_with_cancel(
        addresses,
        Duration::from_secs(1),
        &|| cancelled.get(),
        |_, _| {
            attempts += 1;
            cancelled.set(true);
            Ok(())
        },
    );
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
    assert_eq!(attempts, 1);
}

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../../parity-fixtures/gemini-live-session/transport-address-fallback.json"
    ))
    .unwrap()
}

#[test]
fn shared_contract_interleaves_families_and_tries_later_addresses() {
    let fixture = fixture();
    assert_eq!(
        fixture["totalConnectBudgetMs"],
        TOTAL_BUDGET.as_millis() as u64
    );
    assert_eq!(
        fixture["perAddressLimitMs"],
        PER_ADDRESS_LIMIT.as_millis() as u64
    );
    assert_eq!(fixture["maxAddresses"], MAX_ADDRESSES);
    assert_eq!(fixture["maxResolverWorkers"], MAX_RESOLVERS);
    let case = &fixture["cases"][0];
    let addresses: Vec<SocketAddr> = case["addresses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().parse().unwrap())
        .collect();
    let expected: Vec<SocketAddr> = case["expectedOrder"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().parse().unwrap())
        .collect();
    assert_eq!(interleave(addresses.clone()), expected);
    let mut attempts = Vec::new();
    let result = connect_with(addresses, TOTAL_BUDGET, |address, timeout| {
        attempts.push(*address);
        assert!(timeout <= PER_ADDRESS_LIMIT);
        if attempts.len() == 1 {
            Err(io::Error::new(
                io::ErrorKind::NetworkUnreachable,
                "injected",
            ))
        } else {
            Ok(*address)
        }
    })
    .unwrap();
    assert_eq!(attempts, expected[..2]);
    assert_eq!(result, expected[1]);
}

#[test]
fn exhausted_addresses_return_the_last_error_and_empty_budget_does_not_connect() {
    let addresses: Vec<SocketAddr> = ["192.0.2.1:443", "192.0.2.2:443"]
        .into_iter()
        .map(|value| value.parse().unwrap())
        .collect();
    let mut attempts = 0;
    let error = connect_with::<()>(addresses.clone(), TOTAL_BUDGET, |_, _| {
        attempts += 1;
        Err(io::Error::new(
            if attempts == 1 {
                io::ErrorKind::NetworkUnreachable
            } else {
                io::ErrorKind::ConnectionRefused
            },
            "injected",
        ))
    })
    .unwrap_err();
    assert_eq!(attempts, 2);
    assert_eq!(error.kind(), io::ErrorKind::ConnectionRefused);
    let error = connect_with::<()>(addresses, Duration::ZERO, |_, _| {
        panic!("zero budget connected")
    })
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
}

#[test]
fn timed_out_resolver_retains_capacity_until_it_returns() {
    let _lock = DNS_TEST_LOCK.lock().unwrap();
    let (release, wait) = mpsc::channel();
    let error = resolve_with(&TEST_RESOLVERS, Duration::from_millis(20), move || {
        wait.recv().unwrap();
        Ok(Vec::new())
    })
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert_eq!(TEST_RESOLVERS.load(Ordering::SeqCst), 1);
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while TEST_RESOLVERS.load(Ordering::SeqCst) != 0 {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}

#[test]
fn stalled_resolvers_cannot_create_unbounded_threads() {
    let _lock = DNS_TEST_LOCK.lock().unwrap();
    let mut releases = Vec::new();
    for _ in 0..MAX_RESOLVERS {
        let (release, wait) = mpsc::channel();
        releases.push(release);
        let error = resolve_with(&TEST_RESOLVERS, Duration::from_millis(20), move || {
            wait.recv().unwrap();
            Ok(Vec::new())
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    }
    let error = resolve_with(&TEST_RESOLVERS, Duration::from_secs(1), || {
        panic!("resolver should not start")
    })
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
    for release in releases {
        release.send(()).unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while TEST_RESOLVERS.load(Ordering::SeqCst) != 0 {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}
