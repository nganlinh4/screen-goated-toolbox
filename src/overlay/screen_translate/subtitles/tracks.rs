//! Text-group identity and visibility outlive OCR frames and provider requests.
use super::super::{
    contract::{NormalizedBounds, TranslationRegion},
    render::OverlayMotion,
    runtime,
};
use super::{
    observation::{Frame, Group},
    temporal::Tracker,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

pub(super) struct Track {
    pub id: u64,
    pub generation: u64,
    pub utterance: u64,
    pub group: Group,
    pub frame: Arc<Frame>,
    tracker: Tracker,
    pub source: String,
    pub queued: bool,
    pub in_flight: bool,
    pub translation: Option<TranslationRegion>,
    pub job: Option<u64>,
    pub motion: Option<OverlayMotion>,
    missing_since: Option<u64>,
    pub seen_at: Instant,
}
impl Track {
    pub fn dispatchable(&self) -> bool {
        self.queued
            && !self.in_flight
            && self.missing_since.is_none()
            && self.source == self.group.candidate.source_text
    }
    pub fn ticket(&self) -> Ticket {
        Ticket {
            id: self.id,
            generation: self.generation,
        }
    }
    pub fn hide(&mut self) {
        if let Some(job) = self.job.take() {
            runtime::cancel_job(job);
        }
        self.motion = None;
    }
    fn invalidate(&mut self) {
        self.hide();
        self.generation += 1;
        self.queued = false;
        self.in_flight = false;
        self.translation = None;
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Ticket {
    pub id: u64,
    pub generation: u64,
}
#[derive(Default)]
pub(super) struct Scene {
    pub tracks: Vec<Track>,
    next: u64,
    pub epoch: u64,
    batch: Option<(Vec<Ticket>, Arc<AtomicBool>)>,
}
impl Scene {
    pub fn uncertain(&mut self, now: u64) {
        for t in &mut self.tracks {
            let began = *t.missing_since.get_or_insert(now);
            if now.saturating_sub(began) >= 1000 && !t.source.is_empty() {
                t.invalidate();
                t.source.clear();
                t.tracker.reset();
            }
        }
        self.cancel_obsolete_batch();
    }
    pub fn clear(&mut self, epoch: u64) {
        for t in &mut self.tracks {
            t.hide();
        }
        self.tracks.clear();
        self.epoch = epoch;
        if let Some((_, cancel)) = self.batch.take() {
            cancel.store(true, Ordering::Release);
        }
    }
    pub fn live(&self, ticket: Ticket) -> bool {
        self.tracks
            .iter()
            .any(|t| t.ticket() == ticket && !t.source.is_empty())
    }
    pub fn track_mut(&mut self, ticket: Ticket) -> Option<&mut Track> {
        self.tracks
            .iter_mut()
            .find(|t| t.ticket() == ticket && !t.source.is_empty())
    }
    pub fn batch(&mut self, tickets: Vec<Ticket>, cancel: Arc<AtomicBool>) {
        self.batch = Some((tickets, cancel));
    }
    fn cancel_obsolete_batch(&self) {
        if let Some((tickets, cancel)) = &self.batch
            && !tickets.iter().any(|ticket| self.live(*ticket))
        {
            cancel.store(true, Ordering::Release);
        }
    }
    pub fn observe(&mut self, epoch: u64, frame: Arc<Frame>, now: u64, current: &image::RgbaImage) {
        if self.epoch != epoch {
            self.clear(epoch);
        }
        let mut matched = vec![false; self.tracks.len()];
        let mut uncertain_bounds = frame.uncertain.clone();
        for group in &frame.groups {
            if super::change_detection::cleared(&frame.image, current, group.candidate.bounds) {
                continue;
            }
            if frame.observed_at.elapsed().as_millis() > 1000
                && !super::change_detection::same_edges(
                    &frame.image,
                    current,
                    group.candidate.bounds,
                )
            {
                uncertain_bounds.push(group.candidate.bounds);
                continue;
            }
            let selected = self
                .tracks
                .iter()
                .enumerate()
                .filter(|(i, t)| {
                    !matched[*i] && nearby(t.group.candidate.bounds, group.candidate.bounds)
                })
                .min_by_key(|(_, t)| {
                    (
                        t.group.candidate.source_text != group.candidate.source_text,
                        distance(t.group.candidate.bounds, group.candidate.bounds),
                    )
                })
                .map(|(i, _)| i);
            let index = selected.unwrap_or_else(|| {
                self.next += 1;
                self.tracks.push(Track {
                    id: self.next,
                    generation: 0,
                    utterance: 0,
                    group: group.clone(),
                    frame: Arc::clone(&frame),
                    tracker: Tracker::default(),
                    source: String::new(),
                    queued: false,
                    in_flight: false,
                    translation: None,
                    job: None,
                    motion: None,
                    missing_since: None,
                    seen_at: frame.observed_at,
                });
                matched.push(false);
                self.tracks.len() - 1
            });
            matched[index] = true;
            let t = &mut self.tracks[index];
            t.missing_since = None;
            t.seen_at = frame.observed_at;
            let update = t.tracker.observe(&group.candidate.source_text, now);
            if update.changed {
                t.invalidate();
                if let Some(revision) = update.ready {
                    t.source = revision.text;
                    t.utterance = (t.id << 32) | revision.utterance;
                    t.queued = true;
                }
            } else if t.source == group.candidate.source_text && t.translation.is_some() {
                let old = t.group.candidate.bounds;
                let new = group.candidate.bounds;
                if let Some(motion) = &t.motion {
                    let dx = i32::from(new.left) * frame.image.width() as i32 / 1000
                        - i32::from(old.left) * t.frame.image.width() as i32 / 1000;
                    let dy = i32::from(new.top) * frame.image.height() as i32 / 1000
                        - i32::from(old.top) * t.frame.image.height() as i32 / 1000;
                    if dx != 0 || dy != 0 {
                        motion.shift(dx, dy);
                    }
                }
                if let Some(translation) = &t.translation {
                    let drawn = translation.bounds;
                    let width = drawn.right - drawn.left;
                    let height = drawn.bottom - drawn.top;
                    // Recognition-box jitter must not restart a settled text reveal.
                    // Compare against the rendered size so real accumulated resizing still refits.
                    if width.abs_diff(new.right - new.left) > (width / 4).max(3)
                        || height.abs_diff(new.bottom - new.top) > (height / 4).max(3)
                    {
                        t.hide();
                        t.queued = true;
                    }
                }
            }
            t.group = group.clone();
            t.frame = Arc::clone(&frame);
        }
        for (index, t) in self.tracks.iter_mut().enumerate() {
            if matched[index] {
                continue;
            }
            let uncertain = uncertain_bounds
                .iter()
                .any(|b| overlaps(*b, t.group.candidate.bounds));
            let missing = *t.missing_since.get_or_insert(now);
            let grace = if uncertain { 1000 } else { 100 };
            if now.saturating_sub(missing) >= grace && !t.source.is_empty() {
                t.invalidate();
                t.source.clear();
                t.tracker.reset();
            }
        }
        self.tracks.retain(|t| {
            !t.source.is_empty()
                || t.missing_since
                    .is_none_or(|since| now.saturating_sub(since) < 2000)
        });
        self.cancel_obsolete_batch();
    }
}
fn distance(a: NormalizedBounds, b: NormalizedBounds) -> u32 {
    a.left.abs_diff(b.left) as u32 + a.top.abs_diff(b.top) as u32
}
fn overlaps(a: NormalizedBounds, b: NormalizedBounds) -> bool {
    a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom
}
fn nearby(a: NormalizedBounds, b: NormalizedBounds) -> bool {
    overlaps(a, b) || distance(a, b) <= 80
}
