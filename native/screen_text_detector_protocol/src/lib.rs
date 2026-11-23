//! Bounded protocol shared by the desktop host and the Screen Translate worker.
//!
//! `stream` carries the `sgts-v1` messages. The original request/response
//! detector protocol (`sgtd-v4`) is retired and has no encoder or decoder here.

pub mod recognition;
pub mod stream;

pub const MAX_REGIONS: usize = 2_000;
pub const MAX_REGION_TEXT_BYTES: usize = 2_048;
