//! Streaming worker protocol. Geometry precedes exactly one reading per region.

use crate::{MAX_REGIONS, recognition::Completion};
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

pub const WORKER_VERSION: &str = "4.2.2";
const MAGIC: &[u8; 4] = b"SGTS";
const VERSION: u16 = 1;
const MAX_EVENT: usize = 1024 * 1024;
const MAX_HELLO: usize = 512 * 1024;
/// Largest capture side the worker decodes.
pub const MAX_CAPTURE_SIDE: u32 = 8192;
/// Largest capture area the worker decodes.
pub const MAX_CAPTURE_PIXELS: u64 = 40_000_000;
/// A capture frame must fit any lossless encoding of every decodable image:
/// four bytes per RGBA pixel plus 5% for row filters and container framing.
pub const MAX_CAPTURE_BYTES: usize = MAX_CAPTURE_PIXELS as usize * 4 / 20 * 21;

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

/// Payload bound for one frame kind; writers and readers share it so a frame
/// the host may send is never one the worker refuses.
fn max_payload(kind: u16) -> usize {
    match kind {
        1 => MAX_HELLO,
        2 => MAX_CAPTURE_BYTES,
        101 => MAX_EVENT,
        _ => 0,
    }
}

fn frame(writer: &mut impl Write, id: u64, kind: u16, payload: &[u8]) -> io::Result<()> {
    if id == 0 || payload.len() > max_payload(kind) {
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
    if &header[..4] != MAGIC
        || u16::from_le_bytes(header[4..6].try_into().expect("version")) != VERSION
        || id == 0
        || (kind == 101) != event
        || length > max_payload(kind)
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
    fn capture_frames_fit_every_decodable_image_and_nothing_larger() {
        // A 4K RGBA PNG with fast compression is commonly 20-30 MB.
        let uhd = vec![7_u8; 3840 * 2160 * 3];
        let mut bytes = Vec::new();
        write_request(&mut bytes, 4, &Request::Capture(uhd.clone())).unwrap();
        let (id, request) = read_request(&mut bytes.as_slice()).unwrap();
        assert_eq!(id, 4);
        assert!(matches!(request, Request::Capture(payload) if payload == uhd));

        let raw_rgba = MAX_CAPTURE_PIXELS as usize * 4;
        assert!(MAX_CAPTURE_BYTES > raw_rgba + raw_rgba / 50);
        let oversized = Request::Capture(vec![0; MAX_CAPTURE_BYTES + 1]);
        assert!(write_request(&mut Vec::new(), 5, &oversized).is_err());

        let mut header = Vec::new();
        header.extend_from_slice(b"SGTS\x01\x00\x02\x00");
        header.extend_from_slice(&5_u64.to_le_bytes());
        header.extend_from_slice(&((MAX_CAPTURE_BYTES + 1) as u32).to_le_bytes());
        assert_eq!(
            read_request(&mut header.as_slice()).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn control_requests_carry_no_payload_and_events_use_their_own_kind() {
        let mut cancel = Vec::new();
        cancel.extend_from_slice(b"SGTS\x01\x00\x03\x00");
        cancel.extend_from_slice(&5_u64.to_le_bytes());
        cancel.extend_from_slice(&1_u32.to_le_bytes());
        cancel.push(0);
        assert!(read_request(&mut cancel.as_slice()).is_err());

        let mut event = Vec::new();
        write_event(&mut event, 2, &Event::Finished { count: 0 }).unwrap();
        assert!(read_request(&mut event.as_slice()).is_err());
        let mut request = Vec::new();
        write_request(&mut request, 2, &Request::Cancel).unwrap();
        assert!(read_event(&mut request.as_slice()).is_err());
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
