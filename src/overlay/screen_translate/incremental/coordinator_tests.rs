use super::*;

fn cells(count: usize, bytes: usize) -> Vec<DetectedTextRegion> {
    (0..count)
        .map(|id| DetectedTextRegion {
            id: id as u16,
            bounds: [0, 0, 100, 100].into(),
            source_text: format!("{id}:{}", "x".repeat(bytes)),
            source_alternatives: vec![],
            recognition: Default::default(),
            appearance: None,
        })
        .collect()
}

#[test]
fn modest_complete_scene_is_one_request_without_splitting_ownership() {
    let cells = cells(24, 110);
    let mut c = Coordinator::with_cost(&cells, Cost::default());
    let now = Instant::now();
    for cell in &cells {
        c.completed(cell.id, now);
    }
    let batch = c.take_ready(&cells, now).unwrap();
    assert_eq!(batch.candidates.len(), cells.len());
    assert!(!c.done());
    c.finished(batch.lane, now);
    assert!(c.done());
}

#[test]
fn nearly_finished_ocr_coalesces_and_stalled_ocr_has_a_deadline() {
    let cells = cells(18, 110);
    let mut c = Coordinator::with_cost(&cells, Cost::default());
    let now = Instant::now();
    for cell in cells.iter().take(10) {
        c.completed(cell.id, now);
    }
    assert!(c.take_ready(&cells, now).is_none());
    assert!(
        c.take_ready(&cells, now + Duration::from_millis(100))
            .is_none()
    );
    for cell in cells.iter().skip(10) {
        c.completed(cell.id, now + Duration::from_millis(165));
    }
    assert_eq!(
        c.take_ready(&cells, now + Duration::from_millis(165))
            .unwrap()
            .candidates
            .len(),
        18
    );

    let mut stalled = Coordinator::with_cost(&cells, Cost::default());
    stalled.completed(cells[0].id, now);
    assert!(stalled.take_ready(&cells, now).is_none());
    assert_eq!(
        stalled
            .take_ready(&cells, now + Duration::from_millis(300))
            .unwrap()
            .candidates
            .len(),
        1
    );
}

#[test]
fn overlap_requires_complete_context_and_first_accepted_output() {
    let cells = cells(100, 200);
    let mut c = Coordinator::with_cost(&cells, Cost::default());
    let now = Instant::now();
    for cell in cells.iter().take(40) {
        c.completed(cell.id, now);
    }
    let first = c.take_ready(&cells, now).unwrap();
    assert!(
        c.take_ready(&cells, now + Duration::from_millis(400))
            .is_none()
    );
    c.first_output(
        first.lane,
        first.candidates[0].source_text.len(),
        now + Duration::from_millis(450),
    );
    assert!(
        c.take_ready(&cells, now + Duration::from_millis(450))
            .is_none()
    );
    for cell in cells.iter().skip(40) {
        c.completed(cell.id, now + Duration::from_millis(460));
    }
    let second = c
        .take_ready(&cells, now + Duration::from_millis(460))
        .unwrap();
    assert_ne!(first.lane, second.lane);
    assert!(
        c.take_ready(&cells, now + Duration::from_millis(500))
            .is_none()
    );
    assert!(
        first
            .candidates
            .iter()
            .all(|a| second.candidates.iter().all(|b| a.id != b.id))
    );
}

#[test]
fn repeated_text_waits_for_accepted_terminology() {
    let mut cells = cells(2, 5000);
    cells[1].source_text = cells[0].source_text.clone();
    let mut c = Coordinator::with_cost(&cells, Cost::default());
    let now = Instant::now();
    for cell in &cells {
        c.completed(cell.id, now);
    }
    let first = c.take_ready(&cells, now).unwrap();
    c.first_output(
        first.lane,
        first.candidates[0].source_text.len(),
        now + Duration::from_millis(450),
    );
    assert!(
        c.take_ready(&cells, now + Duration::from_millis(460))
            .is_none()
    );
    c.finished(first.lane, now + Duration::from_millis(700));
    assert_eq!(
        c.take_ready(&cells, now + Duration::from_millis(700))
            .unwrap()
            .candidates[0]
            .id,
        1
    );
}

#[test]
fn dense_completion_retains_every_unit_once_and_bounds_lanes() {
    let mut cells = cells(200, 400);
    cells[0].source_text.clear();
    cells[1].source_text = "oversized".repeat(2000);
    let mut c = Coordinator::with_cost(&cells, Cost::default());
    let start = Instant::now();
    for cell in &cells {
        c.completed(cell.id, start);
    }
    let mut emitted = HashSet::new();
    let mut now = start;
    while !c.done() {
        if let Some(batch) = c.take_ready(&cells, now) {
            for cell in &batch.candidates {
                assert!(emitted.insert(cell.id));
            }
            c.first_output(
                batch.lane,
                batch.candidates[0].source_text.len(),
                now + Duration::from_millis(450),
            );
            c.finished(batch.lane, now + Duration::from_millis(800));
        }
        now += Duration::from_millis(800);
        assert!(now.duration_since(start) < Duration::from_secs(60));
    }
    assert_eq!(emitted, (1..200).collect());
}

#[test]
fn empty_scene_and_unreadable_units_finish_without_requests() {
    let mut cells = cells(3, 1);
    let now = Instant::now();
    let mut empty = Coordinator::new(&[], "empty");
    assert!(empty.take_ready(&[], now).is_none());
    assert!(empty.done());
    for cell in &mut cells {
        cell.source_text.clear();
    }
    let mut c = Coordinator::new(&cells, "empty");
    for cell in &cells {
        c.completed(cell.id, now);
    }
    assert!(c.take_ready(&cells, now).is_none());
    assert!(c.done());
}

#[test]
fn continuation_amortizes_requests_and_out_of_order_completion_releases_only_its_lane() {
    let cells = cells(100, 200);
    let now = Instant::now();
    let mut c = Coordinator::with_cost(&cells, Cost::default());
    for cell in &cells {
        c.completed(cell.id, now);
    }
    let first = c.take_ready(&cells, now).unwrap();
    c.first_output(
        first.lane,
        first.candidates[0].source_text.len(),
        now + Duration::from_millis(450),
    );
    let second = c
        .take_ready(&cells, now + Duration::from_millis(460))
        .unwrap();
    assert!(second.candidates.len() > first.candidates.len() * 2);
    c.finished(second.lane, now + Duration::from_millis(700));
    assert!(!c.done());
    let third = c
        .take_ready(&cells, now + Duration::from_millis(700))
        .unwrap();
    assert_eq!(third.lane, second.lane);
    assert_eq!(
        first.candidates.len() + second.candidates.len() + third.candidates.len(),
        cells.len()
    );
    c.finished(first.lane, now + Duration::from_millis(800));
    assert!(!c.done());
    c.finished(third.lane, now + Duration::from_millis(900));
    assert!(c.done());
}

#[test]
fn measured_cost_excludes_the_first_unit_from_tail_throughput() {
    let cells = cells(2, 798);
    let now = Instant::now();
    let mut c = Coordinator::with_cost(&cells, Cost::default());
    for cell in &cells {
        c.completed(cell.id, now);
    }
    let batch = c.take_ready(&cells, now).unwrap();
    c.first_output(batch.lane, 800, now + Duration::from_millis(650));
    c.finished(batch.lane, now + Duration::from_millis(850));
    assert_eq!(c.cost.first_ms, 450.0);
    assert_eq!(c.cost.bytes_per_ms, 4.0);
}

#[test]
fn many_short_labels_do_not_create_count_based_requests() {
    let cells = cells(150, 8);
    let now = Instant::now();
    let mut c = Coordinator::with_cost(&cells, Cost::default());
    for cell in &cells {
        c.completed(cell.id, now);
    }
    assert_eq!(
        c.take_ready(&cells, now).unwrap().candidates.len(),
        cells.len()
    );
}
