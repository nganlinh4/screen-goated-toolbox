use super::*;

fn make_unavailable(fixture: &mut Fixture) {
    let moved = fixture.root.with_file_name(
        crate::overlay::creation_identity::random_id("sgt-delivery-unavailable-").unwrap(),
    );
    assert_eq!(fixture.root.parent(), moved.parent());
    assert!(!moved.exists());
    std::fs::rename(&fixture.root, &moved).unwrap();
    fixture.root = moved;
}

#[test]
fn unavailable_destination_keeps_the_journal_and_other_handoffs_readable() {
    let mut unavailable = Fixture::new("unavailable");
    let available = Fixture::new("available");
    let journal = available.root.join("deliveries.json");
    let store = DeliveryStore {
        version: VERSION,
        entries: vec![unavailable.record.clone(), available.record.clone()],
        cancellations: Vec::new(),
    };
    let before = serde_json::to_vec(&store).unwrap();
    std::fs::write(&journal, &before).unwrap();
    make_unavailable(&mut unavailable);
    let loaded =
        load_store(&journal).expect("An unavailable destination invalidated unrelated state");
    assert_eq!(loaded.entries.len(), 2);
    assert_eq!(
        loaded.entries[0].dispatch_id,
        unavailable.record.dispatch_id
    );
    assert_eq!(loaded.entries[1].dispatch_id, available.record.dispatch_id);
    assert_eq!(std::fs::read(&journal).unwrap(), before);
    assert!(publication::validate_paths(&unavailable.record).is_err());
    assert!(publication::validate_paths(&available.record).is_ok());
}

#[test]
fn unavailable_destination_is_not_proof_that_an_owned_receipt_disappeared() {
    let mut fixture = Fixture::new("cancel-unavailable");
    fixture.prepare();
    let name = Path::new(&fixture.record.publication_path)
        .file_name()
        .unwrap()
        .to_owned();
    make_unavailable(&mut fixture);
    let owned = fixture.root.join(name);
    let before = std::fs::read(&owned).unwrap();
    assert!(publication::cancel_pre_publication(&fixture.record).is_err());
    assert_eq!(std::fs::read(owned).unwrap(), before);
}

#[test]
fn unavailable_destination_retains_shared_remaining_output_cases() {
    let cases: Value = serde_json::from_str(include_str!(
        "../../../../parity-fixtures/creation/destination-availability.json"
    ))
    .unwrap();
    for case in cases["remainingOutputCases"].as_array().unwrap() {
        let mut fixture = Fixture::new("remaining");
        let bytes = vec![7_u8; case["artifactBytes"].as_u64().unwrap() as usize];
        std::fs::write(&fixture.record.staging_path, bytes).unwrap();
        let artifact = crate::overlay::generation_history::inspect_delivery_artifact(
            &fixture.record.staging_path,
        )
        .unwrap();
        fixture.record.artifact_size_bytes = artifact.size_bytes;
        fixture.record.artifact_sha256 = artifact.sha256;
        if case["prepared"].as_bool().unwrap() {
            fixture.prepare();
        }
        if case["materializedBytes"].is_null() {
            make_unavailable(&mut fixture);
        }
        assert_eq!(
            super::super::reservations::storage_reservation(fixture.record.clone())
                .unwrap()
                .additional_output_bytes,
            case["expectedRemaining"].as_u64().unwrap()
        );
    }
}

#[test]
fn unavailable_destination_does_not_relax_saved_state_integrity() {
    let mut fixture = Fixture::new("integrity");
    let other = Fixture::new("integrity-other");
    fixture.commit_receipt();
    let journal = other.root.join("deliveries.json");
    make_unavailable(&mut fixture);
    for invalid in 0..4 {
        let mut record = fixture.record.clone();
        let mut second = other.record.clone();
        match invalid {
            0 => record.artifact_size_bytes = 0,
            1 => record.staging_path = other.record.staging_path.clone(),
            2 => second.publication_file_identity = record.publication_file_identity.clone(),
            _ => second.output_path = record.output_path.clone(),
        }
        let bytes = serde_json::to_vec(&DeliveryStore {
            version: VERSION,
            entries: vec![record, second],
            cancellations: vec![],
        })
        .unwrap();
        std::fs::write(&journal, &bytes).unwrap();
        assert!(load_store(&journal).is_err(), "invalid case {invalid}");
        assert_eq!(std::fs::read(&journal).unwrap(), bytes);
    }
}

#[path = "destination_tests/native.rs"]
mod native;
