//! Batch current text groups through shared inference; render with independent ownership.
use super::super::{
    contract::{DetectedTextRegion, TranslationDocument, TranslationRegion},
    inference, render, runtime,
};
use super::{Session, history::History, tracks::Ticket};
use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

struct Request {
    ticket: Ticket,
    candidate: DetectedTextRegion,
    utterance: u64,
    observed_at: Instant,
    key: String,
}
struct CacheEntry {
    key: String,
    value: TranslationRegion,
    at: Instant,
}
#[derive(Default)]
struct Cache(VecDeque<CacheEntry>);
impl Cache {
    fn get(&mut self, key: &str) -> Option<TranslationRegion> {
        self.0.retain(|e| e.at.elapsed() < Duration::from_secs(120));
        self.0
            .iter()
            .rev()
            .find(|e| e.key == key)
            .map(|e| e.value.clone())
    }
    fn insert(&mut self, key: String, value: TranslationRegion) {
        if key.len()
            + value
                .translated_segments
                .iter()
                .map(String::len)
                .sum::<usize>()
            > 65_536
        {
            return;
        }
        self.0.retain(|e| e.key != key);
        self.0.push_back(CacheEntry {
            key,
            value,
            at: Instant::now(),
        });
        while self.0.len() > 16
            || self
                .0
                .iter()
                .map(|e| {
                    e.key.len()
                        + e.value
                            .translated_segments
                            .iter()
                            .map(String::len)
                            .sum::<usize>()
                })
                .sum::<usize>()
                > 65_536
        {
            self.0.pop_front();
        }
    }
}

pub(super) fn run(session: Arc<Session>) {
    let mut history = History::default();
    let mut cache = Cache::default();
    loop {
        let mut scene = session.scene.lock().unwrap();
        while !session.stop.load(Ordering::Acquire)
            && !scene.tracks.iter().any(|t| t.dispatchable())
        {
            scene = session.wake.wait(scene).unwrap();
        }
        if session.stop.load(Ordering::Acquire) {
            return;
        }
        let epoch = scene.epoch;
        drop(scene);
        let settings = match crate::APP.lock() {
            Ok(app) => app.config.screen_translate.clone(),
            Err(_) => return,
        };
        history.sync(epoch, &settings);
        let mut scene = session.scene.lock().unwrap();
        if scene.epoch != epoch {
            continue;
        }
        let ready = scene
            .tracks
            .iter()
            .filter(|t| t.dispatchable())
            .take(super::super::contract::MAX_CANDIDATES)
            .map(|t| t.ticket())
            .collect::<Vec<_>>();
        let utterances = ready
            .iter()
            .filter_map(|ticket| {
                scene
                    .tracks
                    .iter()
                    .find(|t| t.ticket() == *ticket)
                    .map(|t| t.utterance)
            })
            .collect::<Vec<_>>();
        let dialogue = history.snapshot_excluding(&utterances, Instant::now());
        let mut candidates = Vec::new();
        let mut requests = Vec::new();
        let neighbors = scene
            .tracks
            .iter()
            .filter(|t| scene.live(t.ticket()))
            .map(|t| t.source.clone())
            .collect::<Vec<_>>();
        for ticket in ready {
            let Some(track) = scene.track_mut(ticket) else {
                continue;
            };
            let key = serde_json::to_string(&(
                epoch,
                &settings.target_language,
                &settings.translation_model,
                &settings.translation_prompt,
                &track.source,
                &dialogue,
                &neighbors,
            ))
            .unwrap();
            track.queued = false;
            if let Some(translation) = track.translation.clone().or_else(|| cache.get(&key)) {
                publish(track, translation, &session);
                continue;
            }
            let mut candidate = track.group.candidate.clone();
            candidate.id = candidates.len() as u16;
            candidates.push(candidate.clone());
            track.in_flight = true;
            requests.push(Request {
                ticket,
                candidate,
                utterance: track.utterance,
                observed_at: track.seen_at,
                key,
            });
        }
        if requests.is_empty() {
            continue;
        }
        let mut context_candidates = candidates.clone();
        for track in &scene.tracks {
            if !scene.live(track.ticket()) || requests.iter().any(|r| r.ticket == track.ticket()) {
                continue;
            }
            let mut candidate = track.group.candidate.clone();
            candidate.id = context_candidates.len() as u16;
            context_candidates.push(candidate);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        scene.batch(
            requests.iter().map(|r| r.ticket).collect(),
            Arc::clone(&cancel),
        );
        let source_frame = scene
            .tracks
            .iter()
            .find(|t| t.ticket() == requests[0].ticket)
            .map(|t| Arc::clone(&t.frame))
            .unwrap();
        drop(scene);
        // All ready groups share one request; the existing chain owns fallback and validation.
        let trace = format!(
            "subtitle-batch-{}-{}-{}",
            epoch, requests[0].ticket.id, requests[0].ticket.generation
        );
        let evidence = super::evidence::begin(&trace, &source_frame, &candidates, &settings);
        crate::log_info!(
            "[Subtitles] dispatch groups={} epoch={epoch}",
            requests.len()
        );
        let result = inference::translate(
            inference::TranslateInput {
                trace_id: &trace,
                target_language: &settings.target_language,
                translation_model: &settings.translation_model,
                translation_prompt: &settings.translation_prompt,
                candidates: &candidates,
                scene: &context_candidates,
                prior_translations: &[],
                dialogue: &dialogue,
            },
            Arc::clone(&cancel),
            |region| {
                let Some(request) = requests.iter().find(|r| r.candidate.id == region.id) else {
                    return;
                };
                let mut scene = session.scene.lock().unwrap();
                if scene.epoch == epoch
                    && !session.stop.load(Ordering::Acquire)
                    && let Some(track) = scene.track_mut(request.ticket)
                {
                    publish(track, region, &session);
                }
            },
        );
        if let Some(evidence) = evidence {
            match &result {
                Ok(outcome) => {
                    evidence.finish_translation(outcome.document.clone(), outcome.warning())
                }
                Err(error) => evidence.fail("subtitle_translation", error),
            }
        }
        let mut scene = session.scene.lock().unwrap();
        if scene.epoch != epoch || session.stop.load(Ordering::Acquire) {
            continue;
        }
        if let Ok(outcome) = &result {
            for region in &outcome.document.regions {
                let Some(request) = requests.iter().find(|r| r.candidate.id == region.id) else {
                    continue;
                };
                if scene.live(request.ticket) {
                    cache.insert(request.key.clone(), region.clone());
                    history.commit(
                        request.utterance,
                        request.observed_at,
                        TranslationDocument {
                            regions: vec![region.clone()],
                        },
                        Instant::now(),
                    );
                }
            }
        } else if let Err(error) = result {
            crate::log_info!("[Subtitles] batch failed: {error:#}");
        }
        for request in requests {
            if let Some(track) = scene
                .tracks
                .iter_mut()
                .find(|t| t.ticket() == request.ticket)
            {
                track.in_flight = false;
                if cancel.load(Ordering::Acquire) && track.translation.is_none() {
                    track.queued = true;
                }
            }
        }
    }
}

fn publish(
    track: &mut super::tracks::Track,
    mut translation: TranslationRegion,
    session: &Session,
) {
    if track
        .translation
        .as_ref()
        .is_some_and(|old| old.translated_segments == translation.translated_segments)
        && track.job.is_some()
    {
        return;
    }
    translation.id = track.group.unit.id;
    translation.member_ids = vec![track.group.unit.id];
    for selection in &mut translation.selections {
        selection.region_id = track.group.unit.id;
        selection.bounds = track.group.candidate.bounds;
    }
    translation.bounds = track.group.candidate.bounds;
    translation.source_text = track.source.clone();
    track.hide();
    let (job, _) = runtime::begin_subtitle_job();
    track.job = Some(job);
    track.translation = Some(translation.clone());
    // Region changes hold view before scene, so never acquire view while holding scene.
    if session.stop.load(Ordering::Acquire) {
        track.hide();
        return;
    }
    let (left, top) = track.frame.origin;
    let image = &track.frame.image;
    let capture = crate::overlay::selection::CapturedRegion {
        image: image.as_ref().clone(),
        left,
        top,
        width: image.width(),
        height: image.height(),
    };
    let trace = format!("screen-translate-{job}");
    crate::overlay::result::latency::begin(&trace);
    crate::overlay::result::latency::mark(&trace, "provider_first_output");
    match render::start(
        job,
        capture,
        Arc::clone(&track.group.sources),
        &trace,
        Some(Arc::from(vec![track.group.unit.clone()])),
        None,
    ) {
        Ok(overlay) => {
            track.motion = Some(overlay.motion());
            std::thread::spawn(move || {
                if let Err(error) = overlay.complete(TranslationDocument {
                    regions: vec![translation],
                }) {
                    runtime::cancel_job(job);
                    crate::log_info!("[Subtitles] renderer failed: {error:#}");
                }
            });
        }
        Err(error) => {
            runtime::cancel_job(job);
            track.job = None;
            crate::log_info!("[Subtitles] renderer failed: {error:#}");
        }
    }
}
