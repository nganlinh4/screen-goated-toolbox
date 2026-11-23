use anyhow::Result;
use serde_json::{Value, json};

use super::super::{
    engine::{DataRejection, RejectionKind},
    protocol::{self, ServerEvent},
    telemetry::{self, Privacy},
};

#[derive(Clone, Copy, Default)]
struct RejectedFrames {
    frames: u64,
    bytes: u64,
}

#[derive(Default)]
pub(super) struct ProviderInput {
    rejected: [RejectedFrames; 3],
}

impl ProviderInput {
    pub(super) fn decode(&mut self, raw: &str) -> Result<Option<Vec<ServerEvent>>> {
        self.decode_with(raw, protocol::parse_server_message, |kind, bytes| {
            telemetry::typed_error(
                "ERR_PROVIDER_FRAME_REJECTED",
                "runtime",
                "provider data was rejected before effects",
                json!({"kind": kind.label(), "bytes": bytes}),
            );
        })
    }

    fn decode_with(
        &mut self,
        raw: &str,
        parse: impl FnOnce(&str) -> Result<Vec<ServerEvent>>,
        mut report: impl FnMut(RejectionKind, usize),
    ) -> Result<Option<Vec<ServerEvent>>> {
        match parse(raw) {
            Ok(events) => Ok(Some(events)),
            Err(error) => {
                let Some(kind) = DataRejection::classify(&error) else {
                    return Err(error);
                };
                let counts = &mut self.rejected[kind as usize];
                let first = counts.frames == 0;
                counts.frames = counts.frames.saturating_add(1);
                counts.bytes = counts
                    .bytes
                    .saturating_add(u64::try_from(raw.len()).unwrap_or(u64::MAX));
                if first {
                    report(kind, raw.len());
                }
                Ok(None)
            }
        }
    }

    fn summary(&self) -> Option<Value> {
        let rejected = RejectionKind::ALL
            .into_iter()
            .filter(|kind| self.rejected[*kind as usize].frames != 0)
            .map(|kind| {
                let counts = self.rejected[kind as usize];
                json!({
                    "kind": kind.label(), "frames": counts.frames, "bytes": counts.bytes,
                })
            })
            .collect::<Vec<_>>();
        (!rejected.is_empty()).then(|| json!({"rejected": rejected}))
    }

    pub(super) fn report_summary(self) {
        if let Some(fields) = self.summary() {
            telemetry::event(
                "provider_frame_rejections",
                "runtime",
                Privacy::Safe,
                fields,
            );
        }
    }
}

#[cfg(test)]
#[path = "provider_input_tests.rs"]
mod tests;
