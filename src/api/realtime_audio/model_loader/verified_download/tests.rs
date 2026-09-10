use super::*;
use std::cell::RefCell;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;

const CONTRACT: FileContract = FileContract {
    name: "model.bin",
    url: "http://127.0.0.1:9/model",
    size_bytes: 3,
    sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "sgt-verified-download-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn file(&self) -> PathBuf {
        self.0.join(CONTRACT.name)
    }

    fn assert_no_partial(&self) {
        assert!(
            fs::read_dir(&self.0)
                .unwrap()
                .all(|entry| { entry.unwrap().file_name() == CONTRACT.name })
        );
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn server(
    body: &'static [u8],
    delay: Duration,
    length: Option<u64>,
) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/model", listener.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            assert!(request.len() < 4096, "test request headers exceed bound");
            let mut byte = [0_u8; 1];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        let length = length
            .map(|value| format!("Content-Length: {value}\r\n"))
            .unwrap_or_default();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\n{length}Connection: close\r\n\r\n"
        )
        .unwrap();
        for byte in body {
            std::thread::sleep(delay);
            if stream.write_all(&[*byte]).is_err() {
                break;
            }
        }
    });
    (url, worker)
}

#[test]
fn verified_download_reports_live_progress_and_publishes_exact_bytes() {
    let dir = TestDirectory::new();
    let (url, worker) = server(b"abc", Duration::from_millis(120), Some(3));
    let events = RefCell::new(Vec::new());
    download_verified_file_with_progress(
        CONTRACT,
        &url,
        &dir.file(),
        &AtomicBool::new(false),
        |done, total| {
            events.borrow_mut().push((done, total));
        },
    )
    .unwrap();
    worker.join().unwrap();
    assert_eq!(fs::read(dir.file()).unwrap(), b"abc");
    let events = events.borrow();
    assert_eq!(events.first(), Some(&(0, 3)));
    assert!(events.iter().any(|&(done, _)| done > 0 && done < 3));
    assert_eq!(events.last(), Some(&(3, 3)));
}

#[test]
fn verified_existing_file_reports_completion_without_a_request() {
    let dir = TestDirectory::new();
    fs::write(dir.file(), b"abc").unwrap();
    let events = RefCell::new(Vec::new());
    download_verified_file_with_progress(
        CONTRACT,
        CONTRACT.url,
        &dir.file(),
        &AtomicBool::new(false),
        |done, total| {
            events.borrow_mut().push((done, total));
        },
    )
    .unwrap();
    assert_eq!(*events.borrow(), vec![(3, 3)]);
}

#[test]
fn cancellation_before_request_does_not_touch_existing_files() {
    let dir = TestDirectory::new();
    fs::write(dir.file(), b"old").unwrap();
    let result = download_verified_file_with_progress(
        CONTRACT,
        CONTRACT.url,
        &dir.file(),
        &AtomicBool::new(true),
        |_, _| panic!("cancelled transfer reported progress"),
    );
    assert!(result.unwrap_err().to_string().contains("cancelled"));
    assert_eq!(fs::read(dir.file()).unwrap(), b"old");
}

#[test]
fn cancellation_during_transfer_preserves_existing_file_and_removes_partial() {
    let dir = TestDirectory::new();
    fs::write(dir.file(), b"old").unwrap();
    let (url, worker) = server(b"abc", Duration::from_millis(120), Some(3));
    let cancelled = AtomicBool::new(false);
    let result =
        download_verified_file_with_progress(CONTRACT, &url, &dir.file(), &cancelled, |done, _| {
            if done > 0 {
                cancelled.store(true, Ordering::Relaxed);
            }
        });
    worker.join().unwrap();
    assert!(result.unwrap_err().to_string().contains("cancelled"));
    assert_eq!(fs::read(dir.file()).unwrap(), b"old");
    dir.assert_no_partial();
}

#[test]
fn wrong_digest_does_not_replace_existing_file() {
    let dir = TestDirectory::new();
    fs::write(dir.file(), b"old").unwrap();
    let (url, worker) = server(b"bad", Duration::ZERO, Some(3));
    let error = download_verified_file_with_progress(
        CONTRACT,
        &url,
        &dir.file(),
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap_err();
    worker.join().unwrap();
    assert!(error.to_string().contains("integrity mismatch"));
    assert_eq!(fs::read(dir.file()).unwrap(), b"old");
    dir.assert_no_partial();
}

#[test]
fn response_length_mismatch_fails_before_writing() {
    let dir = TestDirectory::new();
    let (url, worker) = server(b"abcd", Duration::ZERO, Some(4));
    let error = download_verified_file_with_progress(
        CONTRACT,
        &url,
        &dir.file(),
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap_err();
    worker.join().unwrap();
    assert!(error.to_string().contains("size mismatch"));
    assert!(!dir.file().exists());
}

#[test]
fn response_without_length_still_enforces_exact_inventory_size() {
    let dir = TestDirectory::new();
    let (url, worker) = server(b"abcd", Duration::ZERO, None);
    let error = download_verified_file_with_progress(
        CONTRACT,
        &url,
        &dir.file(),
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap_err();
    worker.join().unwrap();
    assert!(error.to_string().contains("exceeds its declared size"));
    assert!(!dir.file().exists());
    dir.assert_no_partial();
}

#[test]
fn unowned_partial_is_preserved() {
    let dir = TestDirectory::new();
    let partial = dir.file().with_extension("verified-download");
    fs::write(&partial, b"unowned").unwrap();
    let (url, worker) = server(b"abc", Duration::ZERO, Some(3));
    download_verified_file_with_progress(
        CONTRACT,
        &url,
        &dir.file(),
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    worker.join().unwrap();
    assert_eq!(fs::read(&partial).unwrap(), b"unowned");
    assert_eq!(fs::read(dir.file()).unwrap(), b"abc");
}

#[test]
fn concurrent_download_reuses_completed_verified_file() {
    let dir = TestDirectory::new();
    let (url, worker) = server(b"abc", Duration::from_millis(120), Some(3));
    let path = dir.file();
    let other_path = path.clone();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let first = std::thread::spawn(move || {
        download_verified_file_with_progress(
            CONTRACT,
            &url,
            &path,
            &AtomicBool::new(false),
            |done, _| {
                if done == 0 {
                    started_tx.send(()).unwrap();
                }
            },
        )
        .unwrap();
    });
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    download_verified_file_with_progress(
        CONTRACT,
        CONTRACT.url,
        &other_path,
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    first.join().unwrap();
    worker.join().unwrap();
    assert_eq!(fs::read(dir.file()).unwrap(), b"abc");
    dir.assert_no_partial();
}
