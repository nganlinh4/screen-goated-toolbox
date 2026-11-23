use std::cell::RefCell;
use std::io::{Cursor, Read};
use std::sync::atomic::AtomicBool;

use anyhow::{Result, ensure};
use sha2::{Digest as _, Sha256};

use super::{
    BodyValidators, OpenedBody, PinnedBody, discard, discard_owned, download, download_with, fetch,
    part_path, prefetch,
};

fn payload() -> Vec<u8> {
    (0..300_000_u32).map(|value| (value % 251) as u8).collect()
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn validators(etag: &str) -> BodyValidators {
    BodyValidators {
        etag: Some(etag.to_string()),
        last_modified: None,
    }
}

/// Serves `bytes[offset..]`, then fails after `limit` bytes when given.
struct Body {
    bytes: Cursor<Vec<u8>>,
    remaining: Option<usize>,
}

impl Read for Body {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let wanted = match self.remaining {
            Some(0) => return Err(std::io::ErrorKind::ConnectionReset.into()),
            Some(remaining) => remaining.min(buffer.len()),
            None => buffer.len(),
        };
        let read = self.bytes.read(&mut buffer[..wanted])?;
        if let Some(remaining) = &mut self.remaining {
            *remaining -= read;
        }
        Ok(read)
    }
}

fn body(data: &[u8], offset: u64, limit: Option<usize>, etag: &str) -> OpenedBody {
    OpenedBody {
        reader: Box::new(Body {
            bytes: Cursor::new(data[offset as usize..].to_vec()),
            remaining: limit,
        }),
        offset,
        validators: validators(etag),
    }
}

#[test]
fn a_failed_attempt_keeps_its_bytes_and_the_next_attempt_resumes_with_if_range() {
    let data = payload();
    let sha256 = digest(&data);
    let part = part_path("resume-test-model", &sha256).unwrap();
    discard(&part);
    let cancelled = AtomicBool::new(false);

    // Attempt 1: the body breaks after 100 000 bytes and never comes back.
    let mut opened = 0;
    let error = download_with(
        &part,
        "https://example.invalid/model",
        data.len() as u64,
        &sha256,
        &cancelled,
        &|_| {},
        &mut |offset, _| -> Result<OpenedBody> {
            opened += 1;
            ensure!(offset == 0, "the network is down");
            Ok(body(&data, 0, Some(100_000), "\"v1\""))
        },
        &|_| true,
    )
    .unwrap_err();
    assert!(
        format!("{error:#}").contains("could not resume"),
        "{error:#}"
    );
    assert_eq!(std::fs::metadata(&part).unwrap().len(), 100_000);

    // Attempt 2: only the rest is requested, conditional on the same entity.
    let requests = RefCell::new(Vec::new());
    let mut file = download_with(
        &part,
        "https://example.invalid/model",
        data.len() as u64,
        &sha256,
        &cancelled,
        &|_| {},
        &mut |offset, if_range| -> Result<OpenedBody> {
            requests
                .borrow_mut()
                .push((offset, if_range.map(str::to_owned)));
            Ok(body(&data, offset, None, "\"v1\""))
        },
        &|_| true,
    )
    .unwrap();
    assert_eq!(*requests.borrow(), [(100_000, Some("\"v1\"".to_string()))]);
    let mut written = Vec::new();
    file.read_to_end(&mut written).unwrap();
    assert_eq!(written, data);
    drop(file);
    discard(&part);
    assert!(!part.exists());
}

#[test]
fn a_changed_representation_restarts_from_zero() {
    let data = payload();
    let sha256 = digest(&data);
    let part = part_path("restart-test-model", &sha256).unwrap();
    discard(&part);
    let cancelled = AtomicBool::new(false);
    let stale = vec![0xAA_u8; 50_000];
    let attempt = |served: Vec<u8>, etag: &'static str| {
        download_with(
            &part,
            "https://example.invalid/model",
            data.len() as u64,
            &sha256,
            &cancelled,
            &|_| {},
            &mut |offset, _| -> Result<OpenedBody> {
                ensure!(offset == 0, "no resume during the first attempt");
                Ok(body(&served, 0, Some(served.len()), etag))
            },
            &|_| false,
        )
    };
    assert!(attempt(stale, "\"old\"").is_err());
    assert_eq!(std::fs::metadata(&part).unwrap().len(), 50_000);

    // The server no longer has "old" and answers the range with a full 200.
    let file = download_with(
        &part,
        "https://example.invalid/model",
        data.len() as u64,
        &sha256,
        &cancelled,
        &|_| {},
        &mut |offset, if_range| -> Result<OpenedBody> {
            assert_eq!((offset, if_range), (50_000, Some("\"old\"")));
            Ok(body(&data, 0, None, "\"new\""))
        },
        &|_| true,
    )
    .unwrap();
    assert_eq!(file.metadata().unwrap().len(), data.len() as u64);
    drop(file);
    discard(&part);
}

#[test]
fn a_complete_part_with_the_wrong_digest_is_deleted() {
    let data = payload();
    let sha256 = digest(&data);
    let part = part_path("corrupt-test-model", &sha256).unwrap();
    discard(&part);
    let cancelled = AtomicBool::new(false);
    let mut corrupt = data.clone();
    corrupt[10] ^= 1;
    let error = download_with(
        &part,
        "https://example.invalid/model",
        data.len() as u64,
        &sha256,
        &cancelled,
        &|_| {},
        &mut |_, _| Ok(body(&corrupt, 0, None, "\"v1\"")),
        &|_| true,
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("pinned digest"));
    assert!(!part.exists());
    assert!(!part.with_extension("part.json").exists());
}

#[test]
fn part_names_require_a_component_identifier_and_a_digest() {
    assert!(part_path("../escape", &"a".repeat(64)).is_err());
    assert!(part_path("model", "not-a-digest").is_err());
}

#[test]
fn removing_a_component_discards_only_its_partial_downloads() {
    let sha256 = "b".repeat(64);
    let owned = part_path("owned-partial-model", &sha256).unwrap();
    let other = part_path("other-partial-model", &sha256).unwrap();
    for path in [&owned, &owned.with_extension("part.json"), &other] {
        std::fs::write(path, b"partial").unwrap();
    }
    assert!(discard_owned("owned-partial-model").unwrap().is_empty());
    assert!(!owned.exists());
    assert!(!owned.with_extension("part.json").exists());
    assert!(other.exists());
    discard(&other);
}

#[test]
fn plain_http_component_downloads_are_refused() {
    let sha256 = "c".repeat(64);
    let part = part_path("plain-http-model", &sha256).unwrap();
    let error = download(
        &part,
        "http://example.invalid/model",
        1,
        &sha256,
        &AtomicBool::new(false),
        &|_| {},
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("HTTPS"), "{error:#}");
    discard(&part);
}

/// Serves `data` once over loopback HTTP and reports the request it saw.
fn serve_once(data: Vec<u8>) -> (String, std::thread::JoinHandle<()>) {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/component.zip", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap() > 2 {
            line.clear();
        }
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nETag: \"v1\"\r\n\r\n",
            data.len()
        )
        .unwrap();
        stream.write_all(&data).unwrap();
    });
    (url, server)
}

#[test]
fn a_transfer_runs_while_another_thread_holds_the_registry_guard() {
    let data = payload();
    let sha256 = digest(&data);
    let owner = "guard-free-transfer";
    let part = part_path(owner, &sha256).unwrap();
    discard(&part);
    let (url, server) = serve_once(data.clone());
    let body = PinnedBody {
        url: &url,
        size: data.len() as u64,
        sha256: &sha256,
    };
    let cancelled = AtomicBool::new(false);
    let _registry = crate::component_registry::acquire_mutation_guard().unwrap();
    let (done, finished) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        let transfer = scope.spawn(|| {
            let result = prefetch(owner, std::slice::from_ref(&body), &cancelled, &|_, _| {});
            done.send(()).unwrap();
            result
        });
        let waited = finished.recv_timeout(std::time::Duration::from_secs(60));
        cancelled.store(true, std::sync::atomic::Ordering::Release);
        assert!(waited.is_ok(), "the transfer waited for the registry guard");
        transfer.join().unwrap().unwrap();
    });
    server.join().unwrap();

    // The guarded install then finds the verified part without the network.
    let progress = RefCell::new(Vec::new());
    let fetched = fetch(owner, &body, &AtomicBool::new(false), &|done, total| {
        progress.borrow_mut().push((done, total))
    })
    .unwrap();
    assert_eq!(fetched, part);
    assert_eq!(std::fs::read(&part).unwrap(), data);
    assert_eq!(progress.borrow().last(), Some(&(body.size, body.size)));
    discard(&part);
}
