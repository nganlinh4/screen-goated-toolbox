use super::{tests::frame, tracks::Scene};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

fn observe(scene: &mut Scene, lines: &[(&str, u16)], now: u64) {
    let frame = frame(lines);
    scene.observe(1, Arc::clone(&frame), now, &frame.image);
}

#[test]
fn independent_groups_do_not_retranslate_on_background_or_other_group_changes() {
    let mut scene = Scene::default();
    for now in [0, 120] {
        observe(&mut scene, &[("first", 100), ("second", 600)], now);
    }
    let first = scene.tracks[0].ticket();
    let second = scene.tracks[1].ticket();
    for t in &mut scene.tracks {
        t.queued = false;
    }
    for now in [200, 320] {
        observe(&mut scene, &[("replacement", 100), ("second", 600)], now);
    }
    assert!(!scene.live(first));
    assert!(scene.live(second));
    assert!(!scene.tracks[1].queued);
    assert!(scene.tracks[0].queued);
}

#[test]
fn disappearance_invalidates_late_result_without_cancelling_remaining_batch_member() {
    let mut scene = Scene::default();
    for now in [0, 120] {
        observe(&mut scene, &[("first", 100), ("second", 600)], now);
    }
    let tickets = scene.tracks.iter().map(|t| t.ticket()).collect::<Vec<_>>();
    let cancel = Arc::new(AtomicBool::new(false));
    scene.batch(tickets.clone(), Arc::clone(&cancel));
    for now in [200, 320] {
        observe(&mut scene, &[("second", 600)], now);
    }
    assert!(!scene.live(tickets[0]));
    assert!(scene.live(tickets[1]));
    assert!(!cancel.load(Ordering::Acquire));
    for now in [400, 520] {
        observe(&mut scene, &[], now);
    }
    assert!(cancel.load(Ordering::Acquire));
    assert!(scene.track_mut(tickets[0]).is_none());
}

#[test]
fn reordering_ocr_ids_and_small_motion_preserve_identity() {
    let mut scene = Scene::default();
    for now in [0, 120] {
        observe(&mut scene, &[("first", 100), ("second", 600)], now);
    }
    let tickets = scene.tracks.iter().map(|t| t.ticket()).collect::<Vec<_>>();
    for t in &mut scene.tracks {
        t.queued = false;
    }
    observe(&mut scene, &[("second", 605), ("first", 103)], 200);
    assert!(tickets.iter().all(|t| scene.live(*t)));
    assert!(scene.tracks.iter().all(|t| !t.queued));
}

#[test]
fn uncertain_recognition_is_bounded_and_never_dispatches_unseen_source() {
    let mut scene = Scene::default();
    for now in [0, 120] {
        observe(&mut scene, &[("first", 100)], now);
    }
    let ticket = scene.tracks[0].ticket();
    scene.uncertain(200);
    assert!(scene.live(ticket));
    assert!(!scene.tracks[0].dispatchable());
    scene.uncertain(1200);
    assert!(!scene.live(ticket));
}

#[test]
fn typewriter_queue_contains_only_latest_confirmed_revision() {
    let mut scene = Scene::default();
    for step in 0..12 {
        observe(
            &mut scene,
            &[(&"a".repeat(step + 1), 100)],
            step as u64 * 80,
        );
    }
    assert_eq!(scene.tracks.len(), 1);
    assert!(!scene.tracks[0].dispatchable());
    observe(&mut scene, &[(&"a".repeat(12), 100)], 1300);
    assert!(scene.tracks[0].dispatchable());
    assert_eq!(scene.tracks[0].source, "a".repeat(12));
}

#[test]
fn old_ocr_over_unchanged_text_survives_background_motion() {
    let mut frame = frame(&[("stable", 100)]);
    Arc::get_mut(&mut frame).unwrap().observed_at -= std::time::Duration::from_secs(3);
    let mut current = frame.image.as_ref().clone();
    current.put_pixel(900, 180, image::Rgba([255, 0, 0, 255]));
    let mut scene = Scene::default();
    for now in [0, 120] {
        scene.observe(1, Arc::clone(&frame), now, &current);
    }
    let ticket = scene.tracks[0].ticket();
    scene.tracks[0].queued = false;
    scene.observe(1, frame, 4000, &current);
    assert!(scene.live(ticket));
    assert!(!scene.tracks[0].queued);
}

#[test]
fn stale_ambiguous_group_does_not_expire_unchanged_sibling() {
    let mut frame = frame(&[("first", 100), ("second", 600)]);
    Arc::get_mut(&mut frame).unwrap().observed_at -= std::time::Duration::from_secs(3);
    let mut scene = Scene::default();
    for now in [0, 120] {
        scene.observe(1, Arc::clone(&frame), now, &frame.image);
    }
    let tickets = scene.tracks.iter().map(|t| t.ticket()).collect::<Vec<_>>();
    let mut current = frame.image.as_ref().clone();
    current.put_pixel(120, 30, image::Rgba([255, 255, 255, 255]));
    for now in [200, 1200] {
        scene.observe(1, Arc::clone(&frame), now, &current);
    }
    assert!(!scene.live(tickets[0]));
    assert!(scene.live(tickets[1]));
}

#[test]
fn geometry_jitter_preserves_settled_text_but_accumulated_resize_refits_locally() {
    use super::super::contract::{SemanticRole, TranslationRegion};
    let mut scene = Scene::default();
    for now in [0, 120] {
        observe(&mut scene, &[("stable", 100)], now);
    }
    let track = &mut scene.tracks[0];
    track.queued = false;
    track.translation = Some(TranslationRegion {
        id: 0,
        member_ids: vec![0],
        member_joins: vec![],
        selections: vec![],
        semantic_role: SemanticRole::Dialogue,
        source_text: track.source.clone(),
        translated_segments: vec!["translated".into()],
        bounds: track.group.candidate.bounds,
        background_color: None,
        text_color: None,
    });
    for (bottom, expected) in [(280, false), (310, false), (370, true)] {
        let mut next = frame(&[("stable", 100)]);
        Arc::get_mut(&mut next).unwrap().groups[0]
            .candidate
            .bounds
            .bottom = bottom;
        scene.observe(1, Arc::clone(&next), 400, &next.image);
        assert_eq!(scene.tracks[0].queued, expected);
        assert!(scene.tracks[0].translation.is_some());
    }
}
