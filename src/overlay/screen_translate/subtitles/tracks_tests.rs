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
fn motion_preserves_single_pixel_changes_and_negative_screen_origins() {
    use super::super::{geometry::PixelRegion, source_bounds::SourceBounds};
    let mut old = frame(&[("stable", 100)]);
    let old_frame = Arc::make_mut(&mut old);
    old_frame.image = Arc::new(image::RgbaImage::new(7680, 200));
    old_frame.origin = (-1200, 70);
    let bounds = SourceBounds::from_pixels(
        PixelRegion {
            x: 1003,
            y: 20,
            width: 98,
            height: 20,
        },
        7680,
        200,
    )
    .unwrap();
    let moved = SourceBounds::from_pixels(
        PixelRegion {
            x: 1004,
            ..bounds.pixels()
        },
        7680,
        200,
    )
    .unwrap();
    assert_eq!(bounds.prompt(), moved.prompt());
    let mut new = (*old).clone();
    assert_eq!(super::tracks::movement(&old, bounds, &new, moved), (1, 0));
    new.origin = (-1198, 69);
    assert_eq!(super::tracks::movement(&old, bounds, &new, moved), (3, -1));
}

#[test]
fn cached_ocr_frame_cannot_confirm_or_repeat_a_missing_observation() {
    let mut scene = Scene::default();
    let first = frame(&[("visible", 100)]);
    for now in [0, 40, 120, 1000] {
        scene.observe(1, Arc::clone(&first), now, &first.image);
    }
    assert!(scene.tracks[0].source.is_empty());
    assert!(!scene.tracks[0].dispatchable());
    let second = frame(&[("visible", 100)]);
    scene.observe(1, Arc::clone(&second), 1120, &second.image);
    let ticket = scene.tracks[0].ticket();
    assert!(scene.live(ticket));

    let missing = frame(&[]);
    for now in [1200, 1240, 1320, 2000] {
        scene.observe(1, Arc::clone(&missing), now, &missing.image);
    }
    assert!(scene.live(ticket));
    let second_missing = frame(&[]);
    scene.observe(1, Arc::clone(&second_missing), 2120, &second_missing.image);
    assert!(!scene.live(ticket));
}

#[test]
fn repeated_ocr_of_the_same_capture_is_not_new_evidence() {
    let mut scene = Scene::default();
    let first = frame(&[("visible", 100)]);
    scene.observe(1, Arc::clone(&first), 0, &first.image);
    let repeated = Arc::new(super::observation::Frame {
        capture_id: first.capture_id,
        origin: first.origin,
        image: Arc::clone(&first.image),
        groups: first.groups.clone(),
        uncertain: vec![],
        observed_at: first.observed_at,
    });
    scene.observe(1, repeated, 200, &first.image);
    assert!(scene.tracks[0].source.is_empty());
    let next = frame(&[("visible", 100)]);
    scene.observe(1, Arc::clone(&next), 400, &next.image);
    assert_eq!(scene.tracks[0].source, "visible");
}

#[test]
fn late_older_capture_cannot_replace_newer_observation() {
    let mut scene = Scene::default();
    let older = frame(&[("old", 100)]);
    let newer = frame(&[("new", 100)]);
    scene.observe(1, Arc::clone(&newer), 0, &newer.image);
    scene.observe(1, Arc::clone(&older), 200, &older.image);
    assert_eq!(scene.tracks[0].group.candidate.source_text, "new");
    assert!(scene.tracks[0].source.is_empty());
    let confirmation = frame(&[("new", 100)]);
    scene.observe(1, Arc::clone(&confirmation), 400, &confirmation.image);
    assert_eq!(scene.tracks[0].source, "new");
}

#[test]
fn transient_single_read_is_not_queued_before_fresh_correction() {
    let mut scene = Scene::default();
    let transient = frame(&[("a", 100)]);
    for now in [0, 40, 80, 120] {
        scene.observe(1, Arc::clone(&transient), now, &transient.image);
    }
    assert!(!scene.tracks[0].dispatchable());
    observe(&mut scene, &[("ab", 100)], 180);
    assert!(!scene.tracks[0].dispatchable());
    observe(&mut scene, &[("ab", 100)], 560);
    assert_eq!(scene.tracks[0].source, "ab");
    assert!(scene.tracks[0].dispatchable());
}

#[test]
fn visual_clearing_still_expires_a_track_without_fresh_ocr() {
    let mut scene = Scene::default();
    let mut first = frame(&[("visible", 100)]);
    let first_image = Arc::make_mut(&mut Arc::get_mut(&mut first).unwrap().image);
    for y in 30..60 {
        first_image.put_pixel(120, y, image::Rgba([255, 255, 255, 255]));
    }
    scene.observe(1, Arc::clone(&first), 0, &first.image);
    let second = Arc::new(super::observation::Frame {
        capture_id: first.capture_id + 1,
        origin: first.origin,
        image: Arc::new(first.image.as_ref().clone()),
        groups: first.groups.clone(),
        uncertain: vec![],
        observed_at: first.observed_at,
    });
    scene.observe(1, Arc::clone(&second), 120, &second.image);
    let ticket = scene.tracks[0].ticket();
    assert!(scene.live(ticket));
    let cleared = image::RgbaImage::new(1000, 200);
    scene.observe(1, Arc::clone(&second), 200, &cleared);
    scene.observe(1, second, 320, &cleared);
    assert!(!scene.live(ticket));
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
    let mut first = frame(&[("stable", 100)]);
    Arc::get_mut(&mut first).unwrap().observed_at -= std::time::Duration::from_secs(3);
    let mut frame = frame(&[("stable", 100)]);
    Arc::get_mut(&mut frame).unwrap().observed_at -= std::time::Duration::from_secs(3);
    let mut current = frame.image.as_ref().clone();
    current.put_pixel(900, 180, image::Rgba([255, 0, 0, 255]));
    let mut scene = Scene::default();
    scene.observe(1, first, 0, &current);
    scene.observe(1, Arc::clone(&frame), 120, &current);
    let ticket = scene.tracks[0].ticket();
    scene.tracks[0].queued = false;
    scene.observe(1, frame, 4000, &current);
    assert!(scene.live(ticket));
    assert!(!scene.tracks[0].queued);
}

#[test]
fn stale_ambiguous_group_does_not_expire_unchanged_sibling() {
    let mut first = frame(&[("first", 100), ("second", 600)]);
    Arc::get_mut(&mut first).unwrap().observed_at -= std::time::Duration::from_secs(3);
    let mut frame = frame(&[("first", 100), ("second", 600)]);
    Arc::get_mut(&mut frame).unwrap().observed_at -= std::time::Duration::from_secs(3);
    let mut scene = Scene::default();
    scene.observe(1, first, 0, &frame.image);
    scene.observe(1, Arc::clone(&frame), 120, &frame.image);
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
        bounds: track.group.candidate.bounds.prompt(),
        background_color: None,
        text_color: None,
    });
    let pixels = track.group.candidate.bounds.pixels();
    track.rendered_size = Some((pixels.width, pixels.height));
    for (bottom, expected) in [(280, false), (310, false), (370, true)] {
        let mut next = frame(&[("stable", 100)]);
        let candidate = &mut Arc::get_mut(&mut next).unwrap().groups[0].candidate;
        let mut bounds = candidate.bounds.prompt();
        bounds.bottom = bottom;
        candidate.bounds =
            crate::overlay::screen_translate::source_bounds::SourceBounds::from_normalized(
                bounds, 1000, 200,
            )
            .unwrap();
        scene.observe(1, Arc::clone(&next), 400, &next.image);
        assert_eq!(scene.tracks[0].queued, expected);
        assert!(scene.tracks[0].translation.is_some());
    }
}
