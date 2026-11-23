//! Bounded HTTP request framing for the loopback control API.

use std::io::Read;
use std::net::TcpStream;
use std::time::Instant;

use anyhow::{Result, anyhow};

use super::MAXIMUM_REQUEST_BYTES;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum ReadError {
    TooLarge,
    Timeout,
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::TooLarge => "request exceeds byte limit",
            Self::Timeout => "request deadline exceeded",
        })
    }
}

impl std::error::Error for ReadError {}

pub(super) struct Request {
    pub(super) method: String,
    pub(super) path: String,
    pub(super) authorization: Option<String>,
    pub(super) content_type: Option<String>,
    pub(super) body: Vec<u8>,
}

pub(super) fn read_request(stream: &mut TcpStream, deadline: Instant) -> Result<Request> {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 1_024];
    let header_end = loop {
        let count = read_before_deadline(stream, &mut chunk, deadline)?;
        if count == 0 {
            return Err(anyhow!("request ended before headers"));
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.len() > MAXIMUM_REQUEST_BYTES {
            return Err(ReadError::TooLarge.into());
        }
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let headers = std::str::from_utf8(&bytes[..header_end])?;
    let mut lines = headers.split("\r\n");
    let mut request_line = lines.next().unwrap_or_default().split_whitespace();
    let method = request_line.next().unwrap_or_default().to_string();
    let path = request_line.next().unwrap_or_default().to_string();
    let mut content_length = 0_usize;
    let mut authorization = None;
    let mut content_type = None;
    for line in lines.filter(|line| !line.is_empty()) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        match name.trim().to_ascii_lowercase().as_str() {
            "authorization" => authorization = Some(value.trim().to_string()),
            "content-type" => {
                content_type = Some(
                    value
                        .split(';')
                        .next()
                        .unwrap_or_default()
                        .trim()
                        .to_ascii_lowercase(),
                )
            }
            "content-length" => content_length = value.trim().parse()?,
            _ => {}
        }
    }
    let request_end = header_end
        .checked_add(content_length)
        .filter(|&end| end <= MAXIMUM_REQUEST_BYTES)
        .ok_or(ReadError::TooLarge)?;
    while bytes.len() < request_end {
        let remaining = (request_end - bytes.len()).min(chunk.len());
        let count = read_before_deadline(stream, &mut chunk[..remaining], deadline)?;
        if count == 0 {
            return Err(anyhow!("request body ended early"));
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    Ok(Request {
        method,
        path,
        authorization,
        content_type,
        body: bytes[header_end..request_end].to_vec(),
    })
}

fn read_before_deadline(
    stream: &mut TcpStream,
    chunk: &mut [u8],
    deadline: Instant,
) -> Result<usize> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or(ReadError::Timeout)?;
    stream.set_read_timeout(Some(remaining))?;
    let read = stream.read(chunk);
    if Instant::now() >= deadline {
        return Err(ReadError::Timeout.into());
    }
    match read {
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            ) =>
        {
            Err(ReadError::Timeout.into())
        }
        result => result.map_err(Into::into),
    }
}
