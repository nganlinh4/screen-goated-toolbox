//! Structural preset-list changes and index ownership.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresetIndexMove {
    pub from: usize,
    pub to: usize,
}

impl PresetIndexMove {
    pub fn remap(self, index: usize) -> usize {
        if index == self.from {
            self.to
        } else if self.from < self.to && (self.from + 1..=self.to).contains(&index) {
            index - 1
        } else if self.to < self.from && (self.to..self.from).contains(&index) {
            index + 1
        } else {
            index
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresetListChange {
    Moved(PresetIndexMove),
    Removed { index: usize, remaining: usize },
    ActiveProfileChanged,
}

/// Select a surviving row; when the selected row is removed, use its predecessor.
pub fn selection_after_removal(
    index: Option<usize>,
    removed: usize,
    remaining: usize,
) -> Option<usize> {
    let index = index?;
    if remaining == 0 {
        return None;
    }
    Some(
        if index > removed {
            index - 1
        } else if index == removed {
            index.saturating_sub(1)
        } else {
            index
        }
        .min(remaining - 1),
    )
}

/// Map an owned row, discarding its ownership if that row was removed.
pub fn owner_after_removal(
    index: Option<usize>,
    removed: usize,
    remaining: usize,
) -> Option<usize> {
    let index = index?;
    if index == removed {
        return None;
    }
    let mapped = if index > removed { index - 1 } else { index };
    (mapped < remaining).then_some(mapped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_deletion_cases_preserve_the_selected_structural_row() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/preset-system/list-identity.json"
        )))
        .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let index = case["selected"].as_u64().map(|n| n as usize);
            let removed = case["deleted"].as_u64().unwrap() as usize;
            let remaining = case["length"].as_u64().unwrap() as usize - 1;
            let expected = case["expected_selected"].as_u64().map(|n| n as usize);
            assert_eq!(
                selection_after_removal(index, removed, remaining),
                expected,
                "{}",
                case["name"]
            );
        }
    }

    #[test]
    fn removed_row_loses_ownership_and_surviving_rows_follow_their_positions() {
        assert_eq!(owner_after_removal(Some(2), 2, 4), None);
        assert_eq!(owner_after_removal(Some(3), 1, 4), Some(2));
        assert_eq!(owner_after_removal(Some(1), 4, 4), Some(1));
        assert_eq!(owner_after_removal(Some(9), 1, 4), None);
    }

    #[test]
    fn duplicate_preset_ids_do_not_share_row_action_ids() {
        assert_ne!(
            super::super::row_visuals::preset_action_id("duplicate", 0, "delete"),
            super::super::row_visuals::preset_action_id("duplicate", 1, "delete"),
        );
    }
}
