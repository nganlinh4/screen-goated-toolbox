use serde_json::Value;

use super::{CompletionEvidence, EvidenceProvenance, LedgerEntry, MAX_TOTAL_BYTES, compact_entry};

impl CompletionEvidence {
    pub(in crate::overlay::computer_control::uia_task) fn record_dispatch(
        &mut self,
        tool: &str,
        request: &Value,
        result: &Value,
        provenance: EvidenceProvenance,
    ) {
        if !provenance.needs_request_lineage() {
            self.record(tool, result, provenance);
            return;
        }
        let entry = LedgerEntry::new(
            compact_entry(tool, Some(request), result, provenance),
            provenance,
        );
        if entry.len() <= MAX_TOTAL_BYTES {
            self.total_bytes += entry.len();
            self.entries.push(entry);
            self.enforce_bounds();
        }
    }
}
