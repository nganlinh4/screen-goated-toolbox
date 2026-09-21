//! Processing follows source ownership, including units that preserve original pixels.
use super::contract::DetectedTextRegion;
use crate::overlay::result::scene_compositor::ProcessingGlow;
use std::collections::BTreeMap;

pub(super) struct Progress {
    glow: Option<ProcessingGlow>,
    remaining: BTreeMap<u16, Vec<[i32; 4]>>,
}

impl Progress {
    pub(super) fn id(&self) -> Option<u64> {
        self.glow.as_ref().map(ProcessingGlow::id)
    }

    pub(super) fn show(
        rect: windows::Win32::Foundation::RECT,
        enabled: bool,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            glow: enabled.then(|| ProcessingGlow::show(rect)).transpose()?,
            remaining: BTreeMap::new(),
        })
    }

    pub(super) fn geometry(&mut self, sources: &[DetectedTextRegion], width: u32, height: u32) {
        if self.glow.is_none() {
            return;
        }
        self.remaining = sources
            .iter()
            .zip(super::geometry::processing_cells(sources, width, height))
            .map(|(source, rect)| (source.id, vec![rect]))
            .collect();
        self.publish();
    }

    pub(super) fn partition(&mut self, units: &[super::units::Unit]) {
        if self.glow.is_none() {
            return;
        }
        self.remaining = partition(std::mem::take(&mut self.remaining), units);
    }

    pub(super) fn resolved(&mut self, ids: &[u16]) {
        if self.glow.is_none() {
            return;
        }
        let mut changed = false;
        for id in ids {
            changed |= self.remaining.remove(id).is_some();
        }
        if changed {
            self.publish();
        }
    }

    fn publish(&self) {
        if let Some(glow) = &self.glow {
            glow.set_cells(self.remaining.values().flatten().copied().collect());
        }
    }

    pub(super) fn finish(self) {
        if let Some(glow) = self.glow {
            glow.close();
        }
    }
}

fn partition(
    mut sources: BTreeMap<u16, Vec<[i32; 4]>>,
    units: &[super::units::Unit],
) -> BTreeMap<u16, Vec<[i32; 4]>> {
    units
        .iter()
        .map(|unit| {
            (
                unit.id,
                unit.members
                    .iter()
                    .flat_map(|id| sources.remove(id).unwrap_or_default())
                    .collect(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_glow_has_no_native_effect_or_pending_geometry() {
        let mut progress = Progress::show(Default::default(), false).unwrap();
        assert!(progress.id().is_none());
        progress.geometry(&[], 100, 100);
        progress.partition(&[super::super::units::Unit {
            id: 1,
            members: vec![1],
        }]);
        progress.resolved(&[1]);
        assert!(progress.remaining.is_empty());
        progress.finish();
    }

    #[test]
    fn enabled_glow_still_validates_its_native_rectangle() {
        assert!(Progress::show(Default::default(), true).is_err());
    }

    #[test]
    fn completing_a_unit_retires_all_its_sources_and_no_neighbor() {
        let sources = [
            (1, vec![[0, 0, 50, 20]]),
            (2, vec![[0, 30, 80, 20]]),
            (3, vec![[100, 0, 50, 20]]),
        ]
        .into();
        let units = [
            super::super::units::Unit {
                id: 1,
                members: vec![1, 2],
            },
            super::super::units::Unit {
                id: 3,
                members: vec![3],
            },
        ];
        let mut pending = partition(sources, &units);
        assert_eq!(pending.remove(&1).unwrap().len(), 2);
        assert_eq!(
            pending.values().flatten().copied().collect::<Vec<_>>(),
            vec![[100, 0, 50, 20]]
        );
        assert!(pending.remove(&1).is_none());
        pending.remove(&3);
        assert!(pending.is_empty());
    }
}
