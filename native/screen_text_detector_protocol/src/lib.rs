//! Bounded binary protocol shared by the desktop host and screen-text detector.

use std::io::{self, Read, Write};

pub const PROTOCOL_VERSION: u16 = 3;
pub const WORKER_VERSION: &str = "3.1.0";
pub const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_REGIONS: usize = 2_000;
pub const MAX_REGION_TEXT_BYTES: usize = 2_048;
pub const MAX_RECOGNITION_ALTERNATIVES: usize = 9;

const MAGIC: [u8; 4] = *b"SGTD";
const HEADER_BYTES: usize = 20;
const MAX_FRAME_BYTES: usize = MAX_IMAGE_BYTES
    + MAX_REGIONS * (MAX_REGION_TEXT_BYTES * (MAX_RECOGNITION_ALTERNATIVES + 1) + 96);
const MAX_WIDE_UNITS: usize = 16 * 1024;

const HELLO: u16 = 1;
const DETECT: u16 = 2;
const SHUTDOWN: u16 = 3;
const READY: u16 = 101;
const REGIONS: u16 = 102;
const ACK: u16 = 103;
const ERROR: u16 = 199;

#[derive(Clone, Debug, PartialEq)]
pub struct DetectedRegion {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
    pub confidence: f32,
    pub text: String,
    pub text_confidence: f32,
    pub alternatives: Vec<RecognitionAlternative>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecognitionAlternative {
    pub text: String,
    pub confidence: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ClientMessage {
    Hello {
        nonce: [u8; 32],
        runtime_dir: Vec<u16>,
        model_dir: Vec<u16>,
    },
    DetectJpeg(Vec<u8>),
    Shutdown,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ServerMessage {
    Ready {
        nonce: [u8; 32],
        worker_version: String,
    },
    Regions {
        image_width: u32,
        image_height: u32,
        regions: Vec<DetectedRegion>,
    },
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
            runtime_dir,
            model_dir,
        } => {
            payload.extend_from_slice(nonce);
            put_wide(&mut payload, runtime_dir)?;
            put_wide(&mut payload, model_dir)?;
            HELLO
        }
        ClientMessage::DetectJpeg(jpeg) => {
            if jpeg.is_empty() || jpeg.len() > MAX_IMAGE_BYTES {
                return Err(invalid("JPEG exceeds the detector request contract"));
            }
            payload.extend_from_slice(jpeg);
            DETECT
        }
        ClientMessage::Shutdown => SHUTDOWN,
    };
    Ok((kind, payload))
}

fn decode_client(kind: u16, payload: &[u8]) -> io::Result<ClientMessage> {
    match kind {
        HELLO => {
            let mut cursor = Cursor::new(payload);
            let nonce = cursor.array::<32>()?;
            let runtime_dir = cursor.wide()?;
            let model_dir = cursor.wide()?;
            cursor.finish()?;
            Ok(ClientMessage::Hello {
                nonce,
                runtime_dir,
                model_dir,
            })
        }
        DETECT if !payload.is_empty() && payload.len() <= MAX_IMAGE_BYTES => {
            Ok(ClientMessage::DetectJpeg(payload.to_vec()))
        }
        SHUTDOWN if payload.is_empty() => Ok(ClientMessage::Shutdown),
        _ => Err(invalid("unknown detector client message")),
    }
}

fn encode_server(message: &ServerMessage) -> io::Result<(u16, Vec<u8>)> {
    let mut payload = Vec::new();
    let kind = match message {
        ServerMessage::Ready {
            nonce,
            worker_version,
        } => {
            payload.extend_from_slice(nonce);
            put_string(&mut payload, worker_version)?;
            READY
        }
        ServerMessage::Regions {
            image_width,
            image_height,
            regions,
        } => {
            if *image_width == 0
                || *image_height == 0
                || regions.len() > MAX_REGIONS
                || regions
                    .iter()
                    .any(|region| !valid_region(region, *image_width, *image_height))
            {
                return Err(invalid("detector regions exceed the protocol contract"));
            }
            payload.extend_from_slice(&image_width.to_le_bytes());
            payload.extend_from_slice(&image_height.to_le_bytes());
            payload.extend_from_slice(&(regions.len() as u32).to_le_bytes());
            for region in regions {
                payload.extend_from_slice(&region.left.to_le_bytes());
                payload.extend_from_slice(&region.top.to_le_bytes());
                payload.extend_from_slice(&region.right.to_le_bytes());
                payload.extend_from_slice(&region.bottom.to_le_bytes());
                payload.extend_from_slice(&region.confidence.to_le_bytes());
                put_string_bounded(&mut payload, &region.text, MAX_REGION_TEXT_BYTES, true)?;
                payload.extend_from_slice(&region.text_confidence.to_le_bytes());
                payload.extend_from_slice(&(region.alternatives.len() as u32).to_le_bytes());
                for alternative in &region.alternatives {
                    put_string_bounded(
                        &mut payload,
                        &alternative.text,
                        MAX_REGION_TEXT_BYTES,
                        false,
                    )?;
                    payload.extend_from_slice(&alternative.confidence.to_le_bytes());
                }
            }
            REGIONS
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
            worker_version: cursor.string()?,
        },
        REGIONS => {
            let image_width = cursor.u32()?;
            let image_height = cursor.u32()?;
            let count = cursor.u32()? as usize;
            if image_width == 0 || image_height == 0 || count > MAX_REGIONS {
                return Err(invalid("detector response exceeds the protocol contract"));
            }
            let mut regions = Vec::with_capacity(count);
            for _ in 0..count {
                let region = DetectedRegion {
                    left: cursor.u32()?,
                    top: cursor.u32()?,
                    right: cursor.u32()?,
                    bottom: cursor.u32()?,
                    confidence: cursor.f32()?,
                    text: cursor.string_bounded(MAX_REGION_TEXT_BYTES, true)?,
                    text_confidence: cursor.f32()?,
                    alternatives: {
                        let count = cursor.u32()? as usize;
                        if count > MAX_RECOGNITION_ALTERNATIVES {
                            return Err(invalid("too many recognition alternatives"));
                        }
                        let mut alternatives = Vec::with_capacity(count);
                        for _ in 0..count {
                            alternatives.push(RecognitionAlternative {
                                text: cursor.string_bounded(MAX_REGION_TEXT_BYTES, false)?,
                                confidence: cursor.f32()?,
                            });
                        }
                        alternatives
                    },
                };
                if !valid_region(&region, image_width, image_height) {
                    return Err(invalid("detector returned invalid region bounds"));
                }
                regions.push(region);
            }
            ServerMessage::Regions {
                image_width,
                image_height,
                regions,
            }
        }
        ACK if payload.is_empty() => ServerMessage::Ack,
        ERROR => ServerMessage::Error(cursor.string()?),
        _ => return Err(invalid("unknown detector server message")),
    };
    cursor.finish()?;
    Ok(message)
}

fn valid_region(region: &DetectedRegion, width: u32, height: u32) -> bool {
    region.left < region.right
        && region.top < region.bottom
        && region.right <= width
        && region.bottom <= height
        && region.confidence.is_finite()
        && (0.0..=1.0).contains(&region.confidence)
        && region.text.len() <= MAX_REGION_TEXT_BYTES
        && region.text_confidence.is_finite()
        && (0.0..=1.0).contains(&region.text_confidence)
        && region.alternatives.len() <= MAX_RECOGNITION_ALTERNATIVES
        && region.alternatives.iter().all(|alternative| {
            !alternative.text.is_empty()
                && alternative.text.len() <= MAX_REGION_TEXT_BYTES
                && alternative.confidence.is_finite()
                && (0.0..=1.0).contains(&alternative.confidence)
        })
}

fn write_frame(
    writer: &mut impl Write,
    kind: u16,
    request_id: u64,
    payload: &[u8],
) -> io::Result<()> {
    if request_id == 0 || payload.len() > MAX_FRAME_BYTES {
        return Err(invalid("invalid detector frame bounds"));
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
        return Err(invalid("detector frame magic or version mismatch"));
    }
    let kind = u16::from_le_bytes([header[6], header[7]]);
    let request_id = u64::from_le_bytes(header[8..16].try_into().expect("fixed request id"));
    let length = u32::from_le_bytes(header[16..20].try_into().expect("fixed length")) as usize;
    if request_id == 0 || length > MAX_FRAME_BYTES {
        return Err(invalid("invalid detector frame bounds"));
    }
    let mut payload = vec![0_u8; length];
    reader.read_exact(&mut payload)?;
    Ok((kind, request_id, payload))
}

fn put_string(output: &mut Vec<u8>, value: &str) -> io::Result<()> {
    put_string_bounded(output, value, 16 * 1024, false)
}

fn put_string_bounded(
    output: &mut Vec<u8>,
    value: &str,
    maximum: usize,
    allow_empty: bool,
) -> io::Result<()> {
    if (!allow_empty && value.is_empty()) || value.len() > maximum {
        return Err(invalid("detector string exceeds its limit"));
    }
    output.extend_from_slice(&(value.len() as u32).to_le_bytes());
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

fn put_wide(output: &mut Vec<u8>, value: &[u16]) -> io::Result<()> {
    if value.is_empty() || value.len() > MAX_WIDE_UNITS || value.contains(&0) {
        return Err(invalid("detector path exceeds its limit"));
    }
    output.extend_from_slice(&(value.len() as u32).to_le_bytes());
    for unit in value {
        output.extend_from_slice(&unit.to_le_bytes());
    }
    Ok(())
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
            .ok_or_else(|| invalid("truncated detector message"))?;
        let value = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(value)
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
        self.string_bounded(16 * 1024, false)
    }

    fn string_bounded(&mut self, maximum: usize, allow_empty: bool) -> io::Result<String> {
        let length = self.u32()? as usize;
        if (!allow_empty && length == 0) || length > maximum {
            return Err(invalid("detector string exceeds its limit"));
        }
        String::from_utf8(self.take(length)?.to_vec())
            .map_err(|_| invalid("detector string is not UTF-8"))
    }

    fn wide(&mut self) -> io::Result<Vec<u16>> {
        let count = self.u32()? as usize;
        if count == 0 || count > MAX_WIDE_UNITS {
            return Err(invalid("detector path exceeds its limit"));
        }
        let mut value = Vec::with_capacity(count);
        for _ in 0..count {
            value.push(u16::from_le_bytes(
                self.take(2)?.try_into().expect("fixed u16"),
            ));
        }
        if value.contains(&0) {
            return Err(invalid("detector path contains a null unit"));
        }
        Ok(value)
    }

    fn finish(&self) -> io::Result<()> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(invalid("detector message has trailing bytes"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_and_response_round_trip() {
        let hello = ClientMessage::Hello {
            nonce: [7; 32],
            runtime_dir: r"C:\runtime".encode_utf16().collect(),
            model_dir: r"C:\model".encode_utf16().collect(),
        };
        let mut request = Vec::new();
        write_client(&mut request, 3, &hello).unwrap();
        assert_eq!(read_client(&mut request.as_slice()).unwrap(), (3, hello));

        let response = ServerMessage::Regions {
            image_width: 100,
            image_height: 80,
            regions: vec![DetectedRegion {
                left: 2,
                top: 3,
                right: 40,
                bottom: 20,
                confidence: 0.9,
                text: "Settings".to_string(),
                text_confidence: 0.95,
                alternatives: vec![RecognitionAlternative {
                    text: "Settings".to_string(),
                    confidence: 0.95,
                }],
            }],
        };
        let mut wire = Vec::new();
        write_server(&mut wire, 4, &response).unwrap();
        assert_eq!(read_server(&mut wire.as_slice()).unwrap(), (4, response));
    }

    #[test]
    fn invalid_regions_and_oversize_images_are_rejected() {
        let invalid_region = ServerMessage::Regions {
            image_width: 20,
            image_height: 20,
            regions: vec![DetectedRegion {
                left: 10,
                top: 0,
                right: 9,
                bottom: 4,
                confidence: 0.5,
                text: String::new(),
                text_confidence: 0.0,
                alternatives: Vec::new(),
            }],
        };
        assert!(write_server(&mut Vec::new(), 1, &invalid_region).is_err());
        assert!(
            write_client(
                &mut Vec::new(),
                1,
                &ClientMessage::DetectJpeg(vec![0; MAX_IMAGE_BYTES + 1]),
            )
            .is_err()
        );
    }
}
