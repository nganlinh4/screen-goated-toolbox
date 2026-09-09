//! Identity and completion invariants for incremental recognition.

use std::collections::BTreeMap;
use std::io;

use crate::{MAX_REGION_TEXT_BYTES, MAX_REGIONS};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Reading {
    Text(String),
    Unresolved(String),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Completion {
    pub capture_id: u64,
    pub region_id: u32,
    pub reading: Reading,
}

/// A capture owns its region identities before any reader is dispatched.
/// Completion order never determines identity or silently removes a region.
pub struct CaptureReadings {
    capture_id: u64,
    regions: BTreeMap<u32, Option<Reading>>,
    cancelled: bool,
}

impl CaptureReadings {
    pub fn new(capture_id: u64, ids: impl IntoIterator<Item = u32>) -> io::Result<Self> {
        let mut regions = BTreeMap::new();
        for id in ids {
            if regions.len() == MAX_REGIONS || regions.insert(id, None).is_some() {
                return Err(invalid(
                    "duplicate or excessive recognition region identities",
                ));
            }
        }
        Ok(Self {
            capture_id,
            regions,
            cancelled: false,
        })
    }

    pub fn accept(&mut self, completion: Completion) -> io::Result<()> {
        if self.cancelled || completion.capture_id != self.capture_id {
            return Err(invalid("stale recognition completion"));
        }
        let slot = self
            .regions
            .get_mut(&completion.region_id)
            .ok_or_else(|| invalid("unknown recognition region identity"))?;
        if slot.is_some() {
            return Err(invalid("duplicate recognition completion"));
        }
        let text = match &completion.reading {
            Reading::Text(text) | Reading::Unresolved(text) => text,
        };
        if text.trim().is_empty() || text.len() > MAX_REGION_TEXT_BYTES || text.contains('\0') {
            return Err(invalid("recognition completion violates the text bound"));
        }
        *slot = Some(completion.reading);
        Ok(())
    }

    pub fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub fn finish(self) -> io::Result<BTreeMap<u32, Reading>> {
        if self.cancelled || self.regions.values().any(Option::is_none) {
            return Err(invalid("recognition did not resolve every region"));
        }
        Ok(self
            .regions
            .into_iter()
            .map(|(id, reading)| (id, reading.expect("checked completion")))
            .collect())
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn completion(capture_id: u64, region_id: u32) -> Completion {
        Completion {
            capture_id,
            region_id,
            reading: Reading::Text("Text".into()),
        }
    }

    #[test]
    fn out_of_order_results_preserve_identity_and_unresolved_regions() {
        let mut capture = CaptureReadings::new(8, [10, 20]).unwrap();
        capture
            .accept(Completion {
                capture_id: 8,
                region_id: 20,
                reading: Reading::Unresolved("Output limit".into()),
            })
            .unwrap();
        capture.accept(completion(8, 10)).unwrap();
        let result = capture.finish().unwrap();
        assert_eq!(result[&10], Reading::Text("Text".into()));
        assert!(matches!(result[&20], Reading::Unresolved(_)));
    }

    #[test]
    fn rejects_stale_unknown_duplicate_and_missing_completions() {
        let mut capture = CaptureReadings::new(8, [10, 20]).unwrap();
        assert!(capture.accept(completion(7, 10)).is_err());
        assert!(capture.accept(completion(8, 99)).is_err());
        capture.accept(completion(8, 10)).unwrap();
        assert!(capture.accept(completion(8, 10)).is_err());
        assert!(capture.finish().is_err());
    }

    #[test]
    fn cancellation_and_invalid_text_cannot_be_reported_as_success() {
        let mut capture = CaptureReadings::new(8, [10]).unwrap();
        assert!(
            capture
                .accept(Completion {
                    capture_id: 8,
                    region_id: 10,
                    reading: Reading::Text(" ".into())
                })
                .is_err()
        );
        capture.cancel();
        assert!(capture.accept(completion(8, 10)).is_err());
        assert!(capture.finish().is_err());
        assert!(CaptureReadings::new(8, [10, 10]).is_err());
        assert!(CaptureReadings::new(8, 0..=MAX_REGIONS as u32).is_err());
    }
}
