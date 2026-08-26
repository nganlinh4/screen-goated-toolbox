use super::*;

fn budget() -> Budget {
    Budget {
        enforce_managed_cap: true,
        managed_bytes: 2 * 1024 * 1024 * 1024,
        pending_managed_bytes: 0,
        available_bytes: 3 * 1024 * 1024 * 1024,
        free_reserve_bytes: FREE_SPACE_RESERVE_BYTES,
        pending_volume_bytes: 0,
        requested_volume_bytes: 100 * 1024 * 1024,
        requested_managed_bytes: 100 * 1024 * 1024,
        reclaimable_bytes: 0,
    }
}

#[test]
fn planner_preserves_free_reserve_and_managed_cap() {
    assert_eq!(required_reclaim(budget()).unwrap(), 0);
    let mut low = budget();
    low.available_bytes = FREE_SPACE_RESERVE_BYTES + 40;
    low.requested_volume_bytes = 100;
    low.reclaimable_bytes = 60;
    assert_eq!(required_reclaim(low).unwrap(), 60);

    let mut capped = budget();
    capped.managed_bytes = MAX_MANAGED_ARTIFACT_BYTES - 10;
    capped.requested_managed_bytes = 100;
    capped.reclaimable_bytes = 90;
    assert_eq!(required_reclaim(capped).unwrap(), 90);
}

#[test]
fn free_reserve_is_bounded_for_large_volumes() {
    assert_eq!(
        free_reserve_bytes(64 * 1024 * 1024 * 1024),
        FREE_SPACE_RESERVE_BYTES
    );
    assert_eq!(
        free_reserve_bytes(2 * 1024 * 1024 * 1024 * 1024),
        FREE_SPACE_RESERVE_BYTES,
    );
}

#[test]
fn planner_accounts_parallel_reservations_and_rejects_protected_shortfall() {
    let mut constrained = budget();
    constrained.available_bytes = FREE_SPACE_RESERVE_BYTES + 150;
    constrained.pending_volume_bytes = 100;
    constrained.requested_volume_bytes = 100;
    constrained.reclaimable_bytes = 49;
    assert!(required_reclaim(constrained).is_err());
    constrained.reclaimable_bytes = 50;
    assert_eq!(required_reclaim(constrained).unwrap(), 50);
}

#[test]
fn external_destinations_are_capacity_checked_without_managed_pruning() {
    let mut external = budget();
    external.enforce_managed_cap = false;
    external.managed_bytes = u64::MAX;
    external.available_bytes = FREE_SPACE_RESERVE_BYTES;
    external.requested_volume_bytes = 1;
    external.requested_managed_bytes = u64::MAX;
    external.reclaimable_bytes = u64::MAX;
    assert!(required_reclaim(external).is_err());
}

fn entry(id: &str, tool: &str, created_at_ms: u64, output_path: &Path) -> ResultHistoryEntry {
    ResultHistoryEntry {
        id: id.to_string(),
        tool: tool.to_string(),
        source_path: "source.png".to_string(),
        output_path: output_path.to_string_lossy().to_string(),
        output_name: output_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string(),
        created_at_ms,
        artifact_size_bytes: 10,
        artifact_sha256: "0".repeat(64),
        managed_artifact: true,
        metadata: Value::Null,
    }
}

#[test]
fn reclamation_protects_the_newest_result_even_when_entries_are_appended() {
    let oldest_path = PathBuf::from("managed/oldest.glb");
    let newest_path = PathBuf::from("managed/newest.glb");
    let store = ResultHistoryStore {
        entries: vec![
            entry("oldest", "3d", 10, &oldest_path),
            entry("newest", "3d", 20, &newest_path),
        ],
        ..ResultHistoryStore::default()
    };
    let files = HashMap::from([(path_key(&oldest_path), 10), (path_key(&newest_path), 10)]);
    let candidates = reclaimable_entries(&store, &files, &HashSet::new());
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.id.as_str())
            .collect::<Vec<_>>(),
        vec!["oldest"]
    );
}

#[test]
fn product_reservations_separate_final_outputs_from_internal_staging() {
    assert_eq!(
        product_reservation("image", 0, 0).unwrap(),
        Reservation {
            output_bytes: IMAGE_RESULT_RESERVATION_BYTES,
            internal_bytes: IMAGE_RESULT_RESERVATION_BYTES,
        }
    );
    assert_eq!(
        product_reservation("svg", 1024, 1).unwrap().internal_bytes,
        SVG_RESULT_RESERVATION_BYTES + 1024 + SOURCE_PRESENTATION_RESERVATION_BYTES
    );
    assert_eq!(
        product_reservation("3d", 2048, 1).unwrap().internal_bytes,
        THREE_D_RESULT_RESERVATION_BYTES + 2048 + SOURCE_PRESENTATION_RESERVATION_BYTES
    );
    assert_eq!(
        product_reservation("image", 4096, 2)
            .unwrap()
            .internal_bytes,
        IMAGE_RESULT_RESERVATION_BYTES + 4096 + 2 * SOURCE_PRESENTATION_RESERVATION_BYTES
    );
}

#[test]
fn three_d_peak_counts_staging_and_final_on_their_actual_volumes() {
    let reservation = product_reservation("3d", 1024, 1).unwrap();
    assert_eq!(
        requested_root_bytes(reservation, true),
        THREE_D_RESULT_RESERVATION_BYTES + reservation.internal_bytes
    );
    assert_eq!(
        requested_root_bytes(reservation, false),
        reservation.internal_bytes
    );
    assert_eq!(reservation.output_bytes, THREE_D_RESULT_RESERVATION_BYTES);
}

#[test]
fn pressure_watermarks_prune_to_low_without_weakening_hard_limits() {
    let mut pressure = budget();
    pressure.managed_bytes = MANAGED_HIGH_WATERMARK_BYTES + 1;
    pressure.requested_managed_bytes = 0;
    pressure.reclaimable_bytes = MANAGED_HIGH_WATERMARK_BYTES - MANAGED_LOW_WATERMARK_BYTES + 1;
    assert_eq!(
        required_reclaim(pressure).unwrap(),
        pressure.reclaimable_bytes
    );

    pressure.reclaimable_bytes = 0;
    pressure.managed_bytes = MAX_MANAGED_ARTIFACT_BYTES + 1;
    assert!(required_reclaim(pressure).is_err());
}

#[test]
fn managed_scan_counts_hardlinks_once_by_physical_identity() {
    let root = std::env::temp_dir().join(format!(
        "sgt-managed-scan-{}-{}",
        std::process::id(),
        crate::overlay::creation_identity::random_id("case-").unwrap()
    ));
    let vectors = root.join("vectors");
    std::fs::create_dir_all(&vectors).unwrap();
    let first = vectors.join("first.svg");
    let second = vectors.join("second.svg");
    std::fs::write(&first, b"same physical bytes").unwrap();
    std::fs::hard_link(&first, &second).unwrap();

    let files = scan_managed_files(std::slice::from_ref(&root)).unwrap();
    assert_eq!(
        files.total_bytes(),
        std::fs::metadata(&first).unwrap().len()
    );
    assert_eq!(files.paths.len(), 2);
    std::fs::remove_dir_all(root).unwrap();
}
