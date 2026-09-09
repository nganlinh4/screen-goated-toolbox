//! Streaming worker protocol. Geometry precedes exactly one reading per region.

use crate::{MAX_IMAGE_BYTES, MAX_REGIONS, recognition::Completion};
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

pub const WORKER_VERSION: &str = "4.2.2";
const MAGIC: &[u8; 4] = b"SGTS";
const VERSION: u16 = 1;
const MAX_EVENT: usize = 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hello {
    pub nonce: [u8; 32],
    pub runtime_dir: Vec<u16>,
    pub detector_model: Vec<u16>,
    pub reader_catalog: Vec<u16>,
}

#[derive(Clone, Debug)]
pub enum Request {
    Hello(Hello),
    Capture(Vec<u8>),
    Cancel,
    Shutdown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Region {
    pub id: u32,
    /// Clockwise source-image pixel coordinates, beginning at the top left.
    pub quad: [[f32; 2]; 4],
    pub confidence: f32,
}

/// Advisory source-pixel layout. These regions never filter OCR inventory.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayoutRegion {
    pub bounds: [f32; 4],
    pub kind: LayoutKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayoutKind {
    Text,
    VerticalText,
    Table,
    Other,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Event {
    Ready {
        nonce: [u8; 32],
        version: String,
    },
    Geometry {
        width: u32,
        height: u32,
        regions: Vec<Region>,
        layout: Vec<LayoutRegion>,
    },
    Reading {
        completion: Completion,
    },
    Finished {
        count: usize,
    },
    Cancelled,
    Error {
        message: String,
    },
}

pub fn write_request(writer: &mut impl Write, id: u64, request: &Request) -> io::Result<()> {
    match request {
        Request::Hello(hello) => frame(writer, id, 1, &serde_json::to_vec(hello)?),
        Request::Capture(jpeg) if !jpeg.is_empty() => frame(writer, id, 2, jpeg),
        Request::Cancel => frame(writer, id, 3, &[]),
        Request::Shutdown => frame(writer, id, 4, &[]),
        _ => Err(invalid("empty capture")),
    }
}

pub fn read_request(reader: &mut impl Read) -> io::Result<(u64, Request)> {
    let (id, kind, bytes) = read_frame(reader, false)?;
    let request = match kind {
        1 => {
            let hello: Hello = serde_json::from_slice(&bytes)?;
            for path in [
                &hello.runtime_dir,
                &hello.detector_model,
                &hello.reader_catalog,
            ] {
                if path.is_empty() || path.len() > 16384 || path.contains(&0) {
                    return Err(invalid("invalid worker path"));
                }
            }
            Request::Hello(hello)
        }
        2 if !bytes.is_empty() => Request::Capture(bytes),
        3 if bytes.is_empty() => Request::Cancel,
        4 if bytes.is_empty() => Request::Shutdown,
        _ => return Err(invalid("unknown stream request")),
    };
    Ok((id, request))
}

pub fn write_event(writer: &mut impl Write, id: u64, event: &Event) -> io::Result<()> {
    validate(event, id)?;
    let bytes = serde_json::to_vec(event)?;
    if bytes.len() > MAX_EVENT {
        return Err(invalid("oversized stream event"));
    }
    frame(writer, id, 101, &bytes)
}

pub fn read_event(reader: &mut impl Read) -> io::Result<(u64, Event)> {
    let (id, kind, bytes) = read_frame(reader, true)?;
    if kind != 101 {
        return Err(invalid("unknown stream event"));
    }
    let event: Event = serde_json::from_slice(&bytes)?;
    validate(&event, id)?;
    Ok((id, event))
}

fn validate(event: &Event, capture_id: u64) -> io::Result<()> {
    match event {
        Event::Geometry {
            width,
            height,
            regions,
            layout,
        } => {
            if layout.len() > MAX_REGIONS
                || layout.iter().any(|region| {
                    let [left, top, right, bottom] = region.bounds;
                    region.bounds.iter().any(|value| !value.is_finite())
                        || left < 0.0
                        || top < 0.0
                        || right <= left
                        || bottom <= top
                        || right > *width as f32
                        || bottom > *height as f32
                })
            {
                return Err(invalid("invalid advisory layout"));
            }
            if *width == 0
                || *height == 0
                || *width > 8192
                || *height > 8192
                || regions.len() > MAX_REGIONS
            {
                return Err(invalid("invalid geometry dimensions"));
            }
            let mut ids = std::collections::HashSet::new();
            for region in regions {
                if !ids.insert(region.id)
                    || !region.confidence.is_finite()
                    || !(0.0..=1.0).contains(&region.confidence)
                    || region.quad.iter().any(|p| {
                        !p[0].is_finite()
                            || !p[1].is_finite()
                            || p[0] < 0.0
                            || p[1] < 0.0
                            || p[0] > *width as f32
                            || p[1] > *height as f32
                    })
                {
                    return Err(invalid("invalid geometry region"));
                }
                let area: f32 = (0..4)
                    .map(|i| {
                        region.quad[i][0] * region.quad[(i + 1) % 4][1]
                            - region.quad[(i + 1) % 4][0] * region.quad[i][1]
                    })
                    .sum();
                let convex = (0..4).all(|i| {
                    let a = region.quad[i];
                    let b = region.quad[(i + 1) % 4];
                    let c = region.quad[(i + 2) % 4];
                    (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]) > 0.0
                });
                if area < 1.0 || !convex {
                    return Err(invalid("degenerate geometry region"));
                }
            }
        }
        Event::Reading { completion } => {
            let mut state =
                crate::recognition::CaptureReadings::new(capture_id, [completion.region_id])?;
            state.accept(completion.clone())?;
        }
        Event::Finished { count } if *count > MAX_REGIONS => {
            return Err(invalid("invalid completion count"));
        }
        Event::Error { message } if message.is_empty() || message.len() > 16384 => {
            return Err(invalid("invalid worker error"));
        }
        Event::Ready { version, .. } if version.is_empty() || version.len() > 64 => {
            return Err(invalid("invalid worker version"));
        }
        _ => {}
    }
    Ok(())
}

fn frame(writer: &mut impl Write, id: u64, kind: u16, payload: &[u8]) -> io::Result<()> {
    if id == 0 || payload.len() > MAX_IMAGE_BYTES {
        return Err(invalid("invalid stream frame size"));
    }
    writer.write_all(MAGIC)?;
    writer.write_all(&VERSION.to_le_bytes())?;
    writer.write_all(&kind.to_le_bytes())?;
    writer.write_all(&id.to_le_bytes())?;
    writer.write_all(&(payload.len() as u32).to_le_bytes())?;
    writer.write_all(payload)?;
    writer.flush()
}

fn read_frame(reader: &mut impl Read, event: bool) -> io::Result<(u64, u16, Vec<u8>)> {
    let mut header = [0_u8; 20];
    reader.read_exact(&mut header)?;
    let kind = u16::from_le_bytes(header[6..8].try_into().expect("kind"));
    let id = u64::from_le_bytes(header[8..16].try_into().expect("id"));
    let length = u32::from_le_bytes(header[16..20].try_into().expect("length")) as usize;
    let maximum = if event {
        MAX_EVENT
    } else if kind == 1 {
        512 * 1024
    } else {
        MAX_IMAGE_BYTES
    };
    if &header[..4] != MAGIC
        || u16::from_le_bytes(header[4..6].try_into().expect("version")) != VERSION
        || id == 0
        || length > maximum
    {
        return Err(invalid("invalid stream frame header"));
    }
    let mut bytes = vec![0_u8; length];
    reader.read_exact(&mut bytes)?;
    Ok((id, kind, bytes))
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn geometry_rejects_crossed_and_reversed_quads() {
        let quad = [[1.0, 1.0], [9.0, 1.0], [9.0, 9.0], [1.0, 9.0]];
        for points in [
            quad,
            [quad[0], quad[2], quad[1], quad[3]],
            [quad[0], quad[3], quad[2], quad[1]],
        ] {
            let event = Event::Geometry {
                width: 10,
                height: 10,
                regions: vec![Region {
                    id: 1,
                    quad: points,
                    confidence: 0.9,
                }],
                layout: Vec::new(),
            };
            assert_eq!(
                write_event(&mut Vec::new(), 2, &event).is_ok(),
                points == quad
            );
        }
    }
    #[test]
    fn oversized_header_is_rejected_without_reading_payload() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"SGTS\x01\x00\x65\x00");
        bytes.extend_from_slice(&1_u64.to_le_bytes());
        bytes.extend_from_slice(&((MAX_EVENT + 1) as u32).to_le_bytes());
        assert_eq!(
            read_event(&mut bytes.as_slice()).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
    #[test]
    fn stream_round_trip_preserves_capture_and_region_identity() {
        let event = Event::Reading {
            completion: Completion {
                capture_id: 2,
                region_id: 77,
                reading: crate::recognition::Reading::Unresolved("Output incomplete".into()),
            },
        };
        let mut bytes = Vec::new();
        write_event(&mut bytes, 2, &event).unwrap();
        assert!(matches!(
            read_event(&mut bytes.as_slice()).unwrap(),
            (
                2,
                Event::Reading {
                    completion: Completion { region_id: 77, .. }
                }
            )
        ));
        assert!(write_event(&mut Vec::new(), 3, &event).is_err());
    }
}
