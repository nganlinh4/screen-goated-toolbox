use super::*;
use std::cell::RefCell;
use std::io::{BufRead as _, BufReader, Cursor};
use std::net::TcpListener;

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn payload() -> Vec<u8> {
    (0..300_000_u32).map(|value| (value % 251) as u8).collect()
}

/// A reader that yields `remaining` bytes, then fails like a stalled socket
/// (or ends cleanly once the data is exhausted).
struct Stalling {
    inner: Cursor<Vec<u8>>,
    remaining: usize,
}

impl Read for Stalling {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.inner.position() as usize == self.inner.get_ref().len() {
            return Ok(0);
        }
        if self.remaining == 0 {
            return Err(std::io::ErrorKind::TimedOut.into());
        }
        let limit = buffer.len().min(self.remaining);
        let read = self.inner.read(&mut buffer[..limit])?;
        self.remaining -= read;
        Ok(read)
    }
}

#[test]
fn stalled_body_resumes_from_the_received_offset_and_verifies_once() {
    let data = payload();
    let offsets = RefCell::new(Vec::new());
    let mut fetch = |offset: u64| -> Result<Box<dyn Read>> {
        offsets.borrow_mut().push(offset);
        let tail = data[offset as usize..].to_vec();
        let remaining = if offset == 0 { 100_000 } else { tail.len() };
        Ok(Box::new(Stalling {
            inner: Cursor::new(tail),
            remaining,
        }))
    };
    let mut output = Vec::new();
    copy_resuming(
        &mut fetch,
        &mut output,
        data.len() as u64,
        &digest(&data),
        &AtomicBool::new(false),
        &|_| {},
        &|_| true,
    )
    .unwrap();
    assert_eq!(output, data);
    assert_eq!(*offsets.borrow(), vec![0, 100_000]);
}

#[test]
fn resume_budget_is_bounded_without_progress() {
    let data = payload();
    let attempts = RefCell::new(0_u32);
    let mut fetch = |offset: u64| -> Result<Box<dyn Read>> {
        *attempts.borrow_mut() += 1;
        if offset > 0 {
            anyhow::bail!("server ignored Range");
        }
        Ok(Box::new(Cursor::new(data[..1_000].to_vec())))
    };
    let error = copy_resuming(
        &mut fetch,
        &mut Vec::new(),
        data.len() as u64,
        &digest(&data),
        &AtomicBool::new(false),
        &|_| {},
        &|_| true,
    )
    .unwrap_err();
    assert!(
        format!("{error:#}").contains("could not resume at byte 1000"),
        "{error:#}"
    );
    assert_eq!(*attempts.borrow(), 1 + MAX_RESUMES_WITHOUT_PROGRESS);
}

#[test]
fn content_range_must_continue_exactly_at_the_offset() {
    assert!(content_range_matches("bytes 100-999/1000", 100, 1000));
    assert!(!content_range_matches("bytes 0-999/1000", 100, 1000));
    assert!(!content_range_matches("bytes 100-998/999", 100, 1000));
    assert!(!content_range_matches("", 100, 1000));
}

fn read_request(stream: &mut std::net::TcpStream) -> String {
    let mut reader = BufReader::new(stream);
    let mut header = String::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if line == "\r\n" || line.is_empty() {
            return header;
        }
        header.push_str(&line.to_ascii_lowercase());
    }
}

#[test]
fn truncated_http_body_resumes_with_a_range_request() {
    let data = payload();
    let size = data.len();
    let split = 120_000;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/runtime.zip", listener.local_addr().unwrap());
    let served = data.clone();
    let server = std::thread::spawn(move || {
        let (mut first, _) = listener.accept().unwrap();
        assert!(!read_request(&mut first).contains("range:"));
        write!(first, "HTTP/1.1 200 OK\r\nContent-Length: {size}\r\n\r\n").unwrap();
        first.write_all(&served[..split]).unwrap();
        drop(first);

        let (mut second, _) = listener.accept().unwrap();
        let request = read_request(&mut second);
        assert!(
            request.contains(&format!("range: bytes={split}-")),
            "{request}"
        );
        write!(
            second,
            "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {split}-{}/{size}\r\n\r\n",
            size - split,
            size - 1
        )
        .unwrap();
        second.write_all(&served[split..]).unwrap();
    });
    let mut output = Vec::new();
    download_verified(
        &url,
        size as u64,
        &digest(&data),
        &mut output,
        &AtomicBool::new(false),
        &|_| {},
    )
    .unwrap();
    server.join().unwrap();
    assert_eq!(output, data);
}

/// Yields the whole body, then fails instead of reporting a clean end.
struct ResetAfterLastByte(Cursor<Vec<u8>>);

impl Read for ResetAfterLastByte {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        match self.0.read(buffer)? {
            0 => Err(std::io::ErrorKind::ConnectionReset.into()),
            read => Ok(read),
        }
    }
}

#[test]
fn a_reset_after_the_final_byte_completes_without_a_range_past_the_end() {
    let data = payload();
    let offsets = RefCell::new(Vec::new());
    let mut fetch = |offset: u64| -> Result<Box<dyn Read>> {
        offsets.borrow_mut().push(offset);
        ensure!(offset < data.len() as u64, "HTTP 416 range not satisfiable");
        Ok(Box::new(ResetAfterLastByte(Cursor::new(
            data[offset as usize..].to_vec(),
        ))))
    };
    let mut output = Vec::new();
    copy_resuming(
        &mut fetch,
        &mut output,
        data.len() as u64,
        &digest(&data),
        &AtomicBool::new(false),
        &|_| {},
        &|_| true,
    )
    .unwrap();
    assert_eq!(output, data);
    assert_eq!(*offsets.borrow(), [0]);
}
