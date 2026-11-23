use super::*;
use sha2::{Digest as _, Sha256};
use std::io::Read as _;
use std::net::TcpListener;

const BODY: &[u8] = b"pinned cursor bytes";

fn body_pin() -> PinnedBytes {
    let digest = Sha256::digest(BODY)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    PinnedBytes {
        size: BODY.len() as u64,
        sha256: Box::leak(digest.into_boxed_str()),
    }
}

struct TestDirectory(std::path::PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let mut nonce = [0_u8; 16];
        getrandom::fill(&mut nonce).unwrap();
        let path = std::env::temp_dir().join(format!(
            "sgt-cursor-pin-test-{:032x}",
            u128::from_le_bytes(nonce)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Serves `body` once over plain HTTP on loopback and returns its URL.
fn serve_once(body: &'static [u8]) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/cursor.cur", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let mut byte = [0_u8; 1];
        while !request.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap() == 1 {
            request.push(byte[0]);
        }
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(header.as_bytes()).unwrap();
        stream.write_all(body).unwrap();
    });
    (url, server)
}

#[test]
fn stream_accepts_only_the_exact_pinned_bytes() {
    let pin = body_pin();
    let check = |bytes: &[u8]| matches_pin(bytes, &mut std::io::sink(), pin);
    assert_eq!(check(BODY), Ok(true));

    let mut altered = BODY.to_vec();
    altered[0] ^= 1;
    assert_eq!(check(&altered), Ok(false), "same size, different digest");

    let longer = [BODY, b"x"].concat();
    assert!(check(&longer).is_err(), "growth past the pin stops early");
    assert_eq!(
        check(&BODY[..BODY.len() - 1]),
        Ok(false),
        "truncated source"
    );

    let oversized = PinnedBytes {
        size: MAX_CURSOR_SOURCE_BYTES + 1,
        ..pin
    };
    assert!(matches_pin(BODY, &mut std::io::sink(), oversized).is_err());
}

#[test]
fn download_publishes_matching_bytes() {
    let dir = TestDirectory::new();
    let destination = dir.0.join("pointer.cur");
    let (url, server) = serve_once(BODY);
    download_pinned(&url, &destination, body_pin()).unwrap();
    server.join().unwrap();
    assert_eq!(fs::read(&destination).unwrap(), BODY);
    assert!(!destination.with_extension("tmp").exists());
}

#[test]
fn download_never_publishes_tampered_bytes() {
    let dir = TestDirectory::new();
    let destination = dir.0.join("pointer.cur");
    let (url, server) = serve_once(b"pinned cursor bytez");
    let error = download_pinned(&url, &destination, body_pin()).unwrap_err();
    server.join().unwrap();
    assert!(error.contains("SHA-256"), "{error}");
    assert!(!destination.exists());
    assert!(!destination.with_extension("tmp").exists());
}

#[test]
fn download_rejects_a_declared_size_that_differs_from_the_pin() {
    let dir = TestDirectory::new();
    let destination = dir.0.join("pointer.cur");
    let (url, server) = serve_once(b"a much longer replacement payload");
    let error = download_pinned(&url, &destination, body_pin()).unwrap_err();
    server.join().unwrap();
    assert!(error.contains("pinned size"), "{error}");
    assert!(!destination.exists());
}

#[test]
fn cached_archive_is_reused_only_while_it_matches_its_pin() {
    let dir = TestDirectory::new();
    let archive = dir.0.join("_source.zip");
    fs::write(&archive, BODY).unwrap();
    // A matching cache never touches the network.
    ensure_pinned_archive("http://127.0.0.1:9/unreachable", &archive, body_pin()).unwrap();
    assert!(file_matches_pin(&archive, body_pin()).unwrap());

    fs::write(&archive, b"stale unverified archive").unwrap();
    let (url, server) = serve_once(BODY);
    ensure_pinned_archive(&url, &archive, body_pin()).unwrap();
    server.join().unwrap();
    assert_eq!(fs::read(&archive).unwrap(), BODY, "stale cache replaced");
}
