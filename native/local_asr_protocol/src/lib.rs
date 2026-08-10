//! Bounded binary protocol shared by the desktop host and local ASR worker.

use std::io::{self, Read, Write};

pub const PROTOCOL_VERSION: u16 = 1;
pub const WORKER_VERSION: &str = "1.0.0";
pub const EOU_CHUNK_SAMPLES: usize = 2_560;
pub const MAX_TDT_SAMPLES: usize = 640_000;
pub const MAX_FRAME_BYTES: usize = 3 * 1024 * 1024;

const MAGIC: [u8; 4] = *b"SGTA";
const HEADER_BYTES: usize = 20;
const MAX_STRING_BYTES: usize = 16 * 1024;
const MAX_TOKEN_COUNT: usize = 32_000;

const HELLO: u16 = 1;
const EOU_CHUNK: u16 = 2;
const TDT_CHUNK: u16 = 3;
const SHUTDOWN: u16 = 4;
const READY: u16 = 101;
const EOU_TEXT: u16 = 102;
const TDT_TOKENS: u16 = 103;
const ACK: u16 = 104;
const ERROR: u16 = 199;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    RealtimeEou,
    SubtitleTdt,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TimedToken {
    pub start: f32,
    pub end: f32,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ClientMessage {
    Hello {
        nonce: [u8; 32],
        mode: Mode,
        runtime_dir: Vec<u16>,
        model_dir: Vec<u16>,
    },
    EouChunk {
        samples: Vec<f32>,
    },
    TdtChunk {
        sample_rate: u32,
        channels: u16,
        samples: Vec<f32>,
    },
    Shutdown,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ServerMessage {
    Ready {
        nonce: [u8; 32],
        mode: Mode,
        worker_version: String,
    },
    EouText(String),
    TdtTokens(Vec<TimedToken>),
    Ack,
    Error(String),
}

pub fn write_client(
    writer: &mut impl Write,
    request_id: u64,
    message: &ClientMessage,
) -> io::Result<()> {
    let (kind, payload) = encode_client(message)?;
    write_frame(writer, kind, request_id, &payload)
}

pub fn read_client(reader: &mut impl Read) -> io::Result<(u64, ClientMessage)> {
    let (kind, request_id, payload) = read_frame(reader)?;
    Ok((request_id, decode_client(kind, &payload)?))
}

pub fn write_server(
    writer: &mut impl Write,
    request_id: u64,
    message: &ServerMessage,
) -> io::Result<()> {
    let (kind, payload) = encode_server(message)?;
    write_frame(writer, kind, request_id, &payload)
}

pub fn read_server(reader: &mut impl Read) -> io::Result<(u64, ServerMessage)> {
    let (kind, request_id, payload) = read_frame(reader)?;
    Ok((request_id, decode_server(kind, &payload)?))
}

fn encode_client(message: &ClientMessage) -> io::Result<(u16, Vec<u8>)> {
    let mut payload = Vec::new();
    let kind = match message {
        ClientMessage::Hello {
            nonce,
            mode,
            runtime_dir,
            model_dir,
        } => {
            payload.extend_from_slice(nonce);
            payload.push(mode_byte(*mode));
            put_wide(&mut payload, runtime_dir)?;
            put_wide(&mut payload, model_dir)?;
            HELLO
        }
        ClientMessage::EouChunk { samples } => {
            if samples.len() != EOU_CHUNK_SAMPLES {
                return Err(invalid("EOU chunk has an invalid sample count"));
            }
            put_samples(&mut payload, samples)?;
            EOU_CHUNK
        }
        ClientMessage::TdtChunk {
            sample_rate,
            channels,
            samples,
        } => {
            if *sample_rate != 16_000 || *channels != 1 || samples.len() > MAX_TDT_SAMPLES {
                return Err(invalid("TDT audio shape is outside the protocol contract"));
            }
            payload.extend_from_slice(&sample_rate.to_le_bytes());
            payload.extend_from_slice(&channels.to_le_bytes());
            put_samples(&mut payload, samples)?;
            TDT_CHUNK
        }
        ClientMessage::Shutdown => SHUTDOWN,
    };
    Ok((kind, payload))
}

fn decode_client(kind: u16, payload: &[u8]) -> io::Result<ClientMessage> {
    let mut cursor = Cursor::new(payload);
    let message = match kind {
        HELLO => {
            let nonce = cursor.array::<32>()?;
            let mode = parse_mode(cursor.byte()?)?;
            let runtime_dir = cursor.wide()?;
            let model_dir = cursor.wide()?;
            ClientMessage::Hello {
                nonce,
                mode,
                runtime_dir,
                model_dir,
            }
        }
        EOU_CHUNK => {
            let samples = cursor.samples(EOU_CHUNK_SAMPLES)?;
            if samples.len() != EOU_CHUNK_SAMPLES {
                return Err(invalid("EOU chunk has an invalid sample count"));
            }
            ClientMessage::EouChunk { samples }
        }
        TDT_CHUNK => {
            let sample_rate = cursor.u32()?;
            let channels = cursor.u16()?;
            let samples = cursor.samples(MAX_TDT_SAMPLES)?;
            if sample_rate != 16_000 || channels != 1 {
                return Err(invalid("TDT audio shape is outside the protocol contract"));
            }
            ClientMessage::TdtChunk {
                sample_rate,
                channels,
                samples,
            }
        }
        SHUTDOWN if payload.is_empty() => ClientMessage::Shutdown,
        _ => return Err(invalid("unknown client message")),
    };
    cursor.finish()?;
    Ok(message)
}

fn encode_server(message: &ServerMessage) -> io::Result<(u16, Vec<u8>)> {
    let mut payload = Vec::new();
    let kind = match message {
        ServerMessage::Ready {
            nonce,
            mode,
            worker_version,
        } => {
            payload.extend_from_slice(nonce);
            payload.push(mode_byte(*mode));
            put_string(&mut payload, worker_version)?;
            READY
        }
        ServerMessage::EouText(text) => {
            put_string(&mut payload, text)?;
            EOU_TEXT
        }
        ServerMessage::TdtTokens(tokens) => {
            if tokens.len() > MAX_TOKEN_COUNT {
                return Err(invalid("too many timed tokens"));
            }
            payload.extend_from_slice(&(tokens.len() as u32).to_le_bytes());
            for token in tokens {
                validate_token(token)?;
                ensure_payload_room(&payload, 8)?;
                payload.extend_from_slice(&token.start.to_le_bytes());
                payload.extend_from_slice(&token.end.to_le_bytes());
                put_string(&mut payload, &token.text)?;
            }
            TDT_TOKENS
        }
        ServerMessage::Ack => ACK,
        ServerMessage::Error(message) => {
            put_string(&mut payload, message)?;
            ERROR
        }
    };
    Ok((kind, payload))
}

fn decode_server(kind: u16, payload: &[u8]) -> io::Result<ServerMessage> {
    let mut cursor = Cursor::new(payload);
    let message = match kind {
        READY => ServerMessage::Ready {
            nonce: cursor.array::<32>()?,
            mode: parse_mode(cursor.byte()?)?,
            worker_version: cursor.string()?,
        },
        EOU_TEXT => ServerMessage::EouText(cursor.string()?),
        TDT_TOKENS => {
            let count = cursor.u32()? as usize;
            if count > MAX_TOKEN_COUNT {
                return Err(invalid("too many timed tokens"));
            }
            let mut tokens = Vec::with_capacity(count);
            for _ in 0..count {
                let token = TimedToken {
                    start: cursor.f32()?,
                    end: cursor.f32()?,
                    text: cursor.string()?,
                };
                validate_token(&token)?;
                tokens.push(token);
            }
            ServerMessage::TdtTokens(tokens)
        }
        ACK if payload.is_empty() => ServerMessage::Ack,
        ERROR => ServerMessage::Error(cursor.string()?),
        _ => return Err(invalid("unknown server message")),
    };
    cursor.finish()?;
    Ok(message)
}

fn write_frame(
    writer: &mut impl Write,
    kind: u16,
    request_id: u64,
    payload: &[u8],
) -> io::Result<()> {
    if request_id == 0 || payload.len() > MAX_FRAME_BYTES {
        return Err(invalid("invalid frame bounds"));
    }
    writer.write_all(&MAGIC)?;
    writer.write_all(&PROTOCOL_VERSION.to_le_bytes())?;
    writer.write_all(&kind.to_le_bytes())?;
    writer.write_all(&request_id.to_le_bytes())?;
    writer.write_all(&(payload.len() as u32).to_le_bytes())?;
    writer.write_all(payload)?;
    writer.flush()
}

fn read_frame(reader: &mut impl Read) -> io::Result<(u16, u64, Vec<u8>)> {
    let mut header = [0_u8; HEADER_BYTES];
    reader.read_exact(&mut header)?;
    if header[..4] != MAGIC || u16::from_le_bytes([header[4], header[5]]) != PROTOCOL_VERSION {
        return Err(invalid("frame magic or protocol version mismatch"));
    }
    let kind = u16::from_le_bytes([header[6], header[7]]);
    let request_id = u64::from_le_bytes(header[8..16].try_into().expect("fixed request id"));
    let length = u32::from_le_bytes(header[16..20].try_into().expect("fixed length")) as usize;
    if request_id == 0 || length > MAX_FRAME_BYTES {
        return Err(invalid("invalid frame bounds"));
    }
    let mut payload = vec![0_u8; length];
    reader.read_exact(&mut payload)?;
    Ok((kind, request_id, payload))
}

fn put_samples(output: &mut Vec<u8>, samples: &[f32]) -> io::Result<()> {
    if samples
        .iter()
        .any(|sample| !sample.is_finite() || sample.abs() > 4.0)
    {
        return Err(invalid("audio contains an invalid sample"));
    }
    ensure_payload_room(
        output,
        4_usize
            .checked_add(samples.len().saturating_mul(4))
            .ok_or_else(|| invalid("audio size overflow"))?,
    )?;
    output.extend_from_slice(&(samples.len() as u32).to_le_bytes());
    for sample in samples {
        output.extend_from_slice(&sample.to_le_bytes());
    }
    Ok(())
}

fn put_string(output: &mut Vec<u8>, value: &str) -> io::Result<()> {
    if value.len() > MAX_STRING_BYTES {
        return Err(invalid("string exceeds the protocol limit"));
    }
    ensure_payload_room(output, 4 + value.len())?;
    output.extend_from_slice(&(value.len() as u32).to_le_bytes());
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

fn put_wide(output: &mut Vec<u8>, value: &[u16]) -> io::Result<()> {
    if value.is_empty() || value.len() > MAX_STRING_BYTES / 2 || value.contains(&0) {
        return Err(invalid("wide string exceeds the protocol contract"));
    }
    ensure_payload_room(output, 4 + value.len() * 2)?;
    output.extend_from_slice(&(value.len() as u32).to_le_bytes());
    for unit in value {
        output.extend_from_slice(&unit.to_le_bytes());
    }
    Ok(())
}

fn ensure_payload_room(output: &[u8], additional: usize) -> io::Result<()> {
    if output
        .len()
        .checked_add(additional)
        .is_none_or(|length| length > MAX_FRAME_BYTES)
    {
        return Err(invalid("message exceeds the frame limit"));
    }
    Ok(())
}

fn validate_token(token: &TimedToken) -> io::Result<()> {
    if !token.start.is_finite()
        || !token.end.is_finite()
        || token.start < 0.0
        || token.end < token.start
        || token.end > 120.0
    {
        return Err(invalid("timed token has invalid bounds"));
    }
    if token.text.is_empty() || token.text.len() > MAX_STRING_BYTES {
        return Err(invalid("timed token has invalid text"));
    }
    Ok(())
}

fn mode_byte(mode: Mode) -> u8 {
    match mode {
        Mode::RealtimeEou => 1,
        Mode::SubtitleTdt => 2,
    }
}

fn parse_mode(value: u8) -> io::Result<Mode> {
    match value {
        1 => Ok(Mode::RealtimeEou),
        2 => Ok(Mode::SubtitleTdt),
        _ => Err(invalid("unknown ASR mode")),
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, count: usize) -> io::Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(count)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| invalid("truncated message"))?;
        let value = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(value)
    }

    fn byte(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> io::Result<u16> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("fixed u16"),
        ))
    }

    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("fixed u32"),
        ))
    }

    fn f32(&mut self) -> io::Result<f32> {
        Ok(f32::from_le_bytes(
            self.take(4)?.try_into().expect("fixed f32"),
        ))
    }

    fn array<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        Ok(self.take(N)?.try_into().expect("fixed array"))
    }

    fn string(&mut self) -> io::Result<String> {
        let length = self.u32()? as usize;
        if length > MAX_STRING_BYTES {
            return Err(invalid("string exceeds the protocol limit"));
        }
        String::from_utf8(self.take(length)?.to_vec()).map_err(|_| invalid("string is not UTF-8"))
    }

    fn samples(&mut self, maximum: usize) -> io::Result<Vec<f32>> {
        let count = self.u32()? as usize;
        if count > maximum {
            return Err(invalid("audio exceeds the sample limit"));
        }
        let mut samples = Vec::with_capacity(count);
        for _ in 0..count {
            let sample = self.f32()?;
            if !sample.is_finite() || sample.abs() > 4.0 {
                return Err(invalid("audio contains an invalid sample"));
            }
            samples.push(sample);
        }
        Ok(samples)
    }

    fn wide(&mut self) -> io::Result<Vec<u16>> {
        let count = self.u32()? as usize;
        if count == 0 || count > MAX_STRING_BYTES / 2 {
            return Err(invalid("wide string exceeds the protocol contract"));
        }
        let mut value = Vec::with_capacity(count);
        for _ in 0..count {
            value.push(self.u16()?);
        }
        if value.contains(&0) {
            return Err(invalid("wide string contains a null unit"));
        }
        Ok(value)
    }

    fn finish(&self) -> io::Result<()> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(invalid("message has trailing bytes"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_round_trip_preserves_nonce_and_roots() {
        let message = ClientMessage::Hello {
            nonce: [7; 32],
            mode: Mode::SubtitleTdt,
            runtime_dir: r"C:\runtime".encode_utf16().collect(),
            model_dir: r"C:\model".encode_utf16().collect(),
        };
        let mut wire = Vec::new();
        write_client(&mut wire, 4, &message).unwrap();
        assert_eq!(read_client(&mut wire.as_slice()).unwrap(), (4, message));
    }

    #[test]
    fn timed_token_round_trip_is_bounded() {
        let message = ServerMessage::TdtTokens(vec![TimedToken {
            start: 0.2,
            end: 0.8,
            text: "hello".to_string(),
        }]);
        let mut wire = Vec::new();
        write_server(&mut wire, 9, &message).unwrap();
        assert_eq!(read_server(&mut wire.as_slice()).unwrap(), (9, message));
    }

    #[test]
    fn rejects_invalid_audio_and_frame_header() {
        let message = ClientMessage::EouChunk {
            samples: vec![f32::NAN; EOU_CHUNK_SAMPLES],
        };
        assert!(write_client(&mut Vec::new(), 1, &message).is_err());
        let mut bad = vec![0_u8; HEADER_BYTES];
        bad[16..20].copy_from_slice(&((MAX_FRAME_BYTES + 1) as u32).to_le_bytes());
        assert!(read_client(&mut bad.as_slice()).is_err());
    }

    #[test]
    fn rejects_token_payload_before_exceeding_the_frame_limit() {
        let token = TimedToken {
            start: 0.0,
            end: 0.1,
            text: "x".repeat(MAX_STRING_BYTES),
        };
        let message = ServerMessage::TdtTokens(vec![token; 193]);
        assert!(write_server(&mut Vec::new(), 1, &message).is_err());
    }
}
