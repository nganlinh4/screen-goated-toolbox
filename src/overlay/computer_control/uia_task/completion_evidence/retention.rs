use super::EvidenceProvenance;

#[derive(Debug)]
pub(super) struct LedgerEntry {
    text: String,
    provenance: EvidenceProvenance,
}

impl LedgerEntry {
    pub(super) fn new(text: String, provenance: EvidenceProvenance) -> Self {
        Self { text, provenance }
    }

    pub(super) fn as_str(&self) -> &str {
        &self.text
    }

    pub(super) fn len(&self) -> usize {
        self.text.len()
    }

    fn has_surplus(entries: &[Self], provenance: EvidenceProvenance) -> bool {
        entries
            .iter()
            .filter(|entry| entry.provenance == provenance)
            .count()
            > provenance.retention_floor()
    }
}

/// Evict the least trustworthy origin first. Chronological prefix/tail retention
/// is only a tiebreaker, and category floors preserve direct facts alongside
/// recent grounded postconditions.
pub(super) fn eviction_index(entries: &[LedgerEntry], early_slots: usize) -> usize {
    let has_surplus = entries
        .iter()
        .any(|entry| LedgerEntry::has_surplus(entries, entry.provenance));
    let candidates: Vec<_> = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| !has_surplus || LedgerEntry::has_surplus(entries, entry.provenance))
        .map(|(index, _)| index)
        .collect();
    let lowest_rank = candidates
        .iter()
        .map(|index| entries[*index].provenance.retention_rank())
        .min()
        .unwrap_or_default();
    let mut lowest = candidates
        .iter()
        .copied()
        .filter(|index| entries[*index].provenance.retention_rank() == lowest_rank);
    lowest
        .clone()
        .find(|index| *index >= early_slots)
        .or_else(|| lowest.next())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_rank_precedes_chronology() {
        let entries = [
            LedgerEntry::new("advisory-early".into(), EvidenceProvenance::ModelInference),
            LedgerEntry::new("direct".into(), EvidenceProvenance::CapabilityResult),
            LedgerEntry::new("advisory-late".into(), EvidenceProvenance::ModelInference),
        ];
        assert_eq!(eviction_index(&entries, 1), 2);
    }

    #[test]
    fn category_floor_preserves_the_only_direct_fact_from_grounding_churn() {
        let mut entries = vec![LedgerEntry::new(
            "only-direct-fact".into(),
            EvidenceProvenance::CapabilityResult,
        )];
        entries.extend((0..12).map(|index| {
            LedgerEntry::new(
                format!("grounded-{index}"),
                EvidenceProvenance::GroundedSurface,
            )
        }));
        assert_ne!(eviction_index(&entries, 4), 0);
    }

    #[test]
    fn category_floor_reserves_two_grounded_postconditions_from_direct_churn() {
        let mut entries = vec![
            LedgerEntry::new("grounded-a".into(), EvidenceProvenance::GroundedSurface),
            LedgerEntry::new("grounded-b".into(), EvidenceProvenance::GroundedSurface),
        ];
        entries.extend((0..11).map(|index| {
            LedgerEntry::new(
                format!("direct-{index}"),
                EvidenceProvenance::CapabilityResult,
            )
        }));
        assert!(eviction_index(&entries, 4) >= 2);
    }
}
