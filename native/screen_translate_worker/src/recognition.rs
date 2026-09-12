use anyhow::{Context, Result};
use image::RgbImage;
use sgt_screen_text_detector_protocol::{
    recognition::{Completion, Reading},
    stream::Region,
};
use std::{path::Path, sync::atomic::AtomicBool};

mod catalog;
mod lines;
mod pipeline;
mod reader;
mod router;
mod router_preprocess;

pub(crate) struct Recognizer {
    router: [router::Router; 2],
    readers: Vec<(String, reader::Reader)>,
    alphabets: Vec<std::collections::HashSet<char>>,
}

impl Recognizer {
    pub(crate) fn load(path: &Path) -> Result<Self> {
        let catalog = catalog::Catalog::read(path)?;
        let root = path.parent().context("missing reader package root")?;
        let router = [
            router::Router::load(root, &catalog)?,
            router::Router::load(root, &catalog)?,
        ];
        let readers = catalog
            .readers
            .iter()
            .map(|spec| Ok((spec.id.clone(), reader::Reader::load(root, spec)?)))
            .collect::<Result<Vec<_>>>()?;
        let alphabets = readers
            .iter()
            .map(|(_, reader)| reader.alphabet())
            .collect();
        Ok(Self {
            router,
            readers,
            alphabets,
        })
    }

    pub(crate) fn recognize(
        &mut self,
        capture_id: u64,
        image: &RgbImage,
        regions: &[Region],
        cancel: &AtomicBool,
        mut emit: impl FnMut(&Completion) -> Result<()>,
    ) -> Result<()> {
        pipeline::recognize(
            &mut self.router,
            &mut self.readers,
            &self.alphabets,
            pipeline::Capture {
                capture_id,
                image,
                regions,
                cancel,
            },
            &mut emit,
        )
    }
}

fn prefer_reading(
    primary: Completion,
    alternative: Completion,
    alphabet: &std::collections::HashSet<char>,
) -> Completion {
    match (&primary.reading, &alternative.reading) {
        (Reading::Unresolved(_), _) => alternative,
        (_, Reading::Text(text))
            if text
                .chars()
                .any(|c| c.is_alphabetic() && !alphabet.contains(&c)) =>
        {
            alternative
        }
        _ => primary,
    }
}

fn nearest_neighbors(source: usize, regions: &[Region]) -> [Option<usize>; 4] {
    let bounds = |region: &Region| {
        [
            region
                .quad
                .iter()
                .map(|p| p[0])
                .fold(f32::INFINITY, f32::min),
            region
                .quad
                .iter()
                .map(|p| p[1])
                .fold(f32::INFINITY, f32::min),
            region
                .quad
                .iter()
                .map(|p| p[0])
                .fold(f32::NEG_INFINITY, f32::max),
            region
                .quad
                .iter()
                .map(|p| p[1])
                .fold(f32::NEG_INFINITY, f32::max),
        ]
    };
    let [left, top, right, bottom] = bounds(&regions[source]);
    let mut nearest = [(f32::INFINITY, None); 4];
    for (index, region) in regions.iter().enumerate() {
        if index == source {
            continue;
        }
        let [l, t, r, b] = bounds(region);
        let limit = 4.0 * (bottom - top).max(b - t);
        let vertical_overlap = top.max(t) < bottom.min(b);
        let horizontal_overlap = left.max(l) < right.min(r);
        // Expanded locator boxes may overlap adjacent lines. Direction belongs
        // to their centers, not to an artificial gap between expanded edges.
        let distances = [
            (vertical_overlap && l + r < left + right).then_some(((left + right) - (l + r)) * 0.5),
            (vertical_overlap && l + r > left + right).then_some(((l + r) - (left + right)) * 0.5),
            (horizontal_overlap && t + b < top + bottom)
                .then_some(((top + bottom) - (t + b)) * 0.5),
            (horizontal_overlap && t + b > top + bottom)
                .then_some(((t + b) - (top + bottom)) * 0.5),
        ];
        for (slot, distance) in nearest.iter_mut().zip(distances) {
            if let Some(distance) = distance
                && distance <= limit
                && distance < slot.0
            {
                *slot = (distance, Some(index));
            }
        }
    }
    nearest.map(|(_, index)| index)
}

fn local_support(source: usize, alternative: usize, routes: &[usize], regions: &[Region]) -> bool {
    nearest_neighbors(source, regions)
        .into_iter()
        .flatten()
        .any(|index| routes[index] == alternative)
}

fn supported_alternative(source: usize, routes: &[usize], regions: &[Region]) -> Option<usize> {
    let mut votes = std::collections::BTreeMap::<usize, usize>::new();
    let mut seen = std::collections::HashSet::new();
    for i in nearest_neighbors(source, regions).into_iter().flatten() {
        if seen.insert(i) && routes[i] != 0 && routes[i] != routes[source] {
            *votes.entry(routes[i]).or_default() += 1;
        }
    }
    votes
        .into_iter()
        .filter(|&(_, count)| count >= if routes[source] == 0 { 1 } else { 2 })
        .max_by_key(|&(reader, count)| (count, std::cmp::Reverse(reader)))
        .map(|(reader, _)| reader)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn region(id: u32, x: f32, y: f32) -> Region {
        Region {
            id,
            quad: [[x, y], [x + 40.0, y], [x + 40.0, y + 20.0], [x, y + 20.0]],
            confidence: 1.0,
        }
    }
    #[test]
    fn recovery_requires_nearby_directional_support_not_screen_majority() {
        let regions = [
            region(1, 0.0, 0.0),
            region(2, 0.0, 30.0),
            region(3, 0.0, 500.0),
        ];
        assert!(local_support(0, 1, &[0, 1, 2], &regions));
        assert!(!local_support(0, 2, &[0, 1, 2], &regions));
        assert!(!local_support(0, 1, &[0, 0, 1], &regions));
    }

    #[test]
    fn expanded_adjacent_lines_still_supply_script_evidence() {
        let regions = [region(1, 0.0, 0.0), region(2, 0.0, 16.0)];
        assert!(local_support(0, 1, &[0, 1], &regions));
        assert_eq!(supported_alternative(0, &[0, 1], &regions), Some(1));
    }

    #[test]
    fn one_neighbor_cannot_count_as_two_independent_script_votes() {
        let regions = [region(1, 0.0, 0.0), region(2, 5.0, 16.0)];
        assert_eq!(supported_alternative(0, &[2, 1], &regions), None);
    }

    #[test]
    fn alternative_must_add_missing_alphabet_evidence_to_replace_text() {
        let alphabet = "abc012".chars().collect();
        let reading = |text: &str| Completion {
            capture_id: 1,
            region_id: 2,
            reading: Reading::Text(text.into()),
        };
        assert!(
            matches!(prefer_reading(reading("abc"),reading("ab"),&alphabet).reading, Reading::Text(s) if s == "abc")
        );
        assert!(
            matches!(prefer_reading(reading("a"),reading("ñ"),&alphabet).reading, Reading::Text(s) if s == "ñ")
        );
    }
}
