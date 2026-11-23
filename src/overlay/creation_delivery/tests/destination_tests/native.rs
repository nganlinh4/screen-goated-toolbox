use super::*;
use std::process::{Command, Stdio};

const CASE_ENV: &str = "SGT_TEST_DESTINATION_CASE";
const TEST: &str = "overlay::creation_delivery::tests::destination_tests::native::unavailable_destination_preserves_independent_admission_and_cancellation";

fn exercise(case: &Value) {
    let mut fixtures = [Fixture::new("first"), Fixture::new("second")];
    for fixture in &mut fixtures {
        crate::overlay::creation_intent_journal::record("image", &fixture.record.job_id, &fixture.record.dispatch_id,
            serde_json::json!({"outputDir":Path::new(&fixture.record.staging_path).parent().unwrap(),
                "finalOutputDir":std::fs::canonicalize(&fixture.root).unwrap(), "outputName":fixture.record.output_name})).unwrap();
        fixture.record.request_fingerprint = crate::overlay::creation_intent_journal::load("image")
            .unwrap()
            .into_iter()
            .find(|intent| intent.dispatch_id == fixture.record.dispatch_id)
            .unwrap()
            .arguments_fingerprint;
    }
    let path = journal_path();
    let store = DeliveryStore {
        version: VERSION,
        entries: fixtures.iter().map(|f| f.record.clone()).collect(),
        cancellations: vec![],
    };
    save_store(&path, &store).unwrap();
    let mut offline = 0;
    let mut ready = 0;
    let mut original = PathBuf::new();
    for (index, available) in case["available"].as_array().unwrap().iter().enumerate() {
        if available.as_bool().unwrap() {
            ready = index;
        } else {
            offline = index;
            original = fixtures[index].root.clone();
            make_unavailable(&mut fixtures[index]);
        }
    }
    let saved_request = crate::overlay::creation_intent_journal::load("image").unwrap();
    let admitted = crate::overlay::generation_history::admit_and_record(
        "image",
        &fixtures[ready].root,
        0,
        0,
        || Ok(true),
    )
    .unwrap();
    assert!(admitted);
    assert_eq!(
        serde_json::to_vec(&crate::overlay::creation_intent_journal::load("image").unwrap())
            .unwrap(),
        serde_json::to_vec(&saved_request).unwrap()
    );
    let record = &fixtures[offline].record;
    assert_eq!(
        cancel_dispatch(CancelledDelivery {
            product: "image",
            job_id: record.job_id.clone(),
            dispatch_id: record.dispatch_id.clone(),
            request_fingerprint: record.request_fingerprint.clone(),
            output_name: record.output_name.clone()
        })
        .unwrap(),
        CancellationOutcome::Cancelled
    );
    let pending = reconcile_product("image").unwrap();
    assert!(pending.contains(&fixtures[offline].record.job_id));
    assert!(!pending.contains(&fixtures[ready].record.job_id));
    assert_eq!(
        std::fs::read(&fixtures[ready].record.output_path).unwrap(),
        b"published-image"
    );
    let retained = load_store(&path).unwrap();
    assert_eq!(retained.entries.len(), 1);
    assert_eq!(retained.cancellations.len(), 1);
    assert_eq!(
        retained.entries[0].dispatch_id,
        fixtures[offline].record.dispatch_id
    );
    let ready_original = fixtures[ready].root.clone();
    make_unavailable(&mut fixtures[ready]);
    let history = crate::overlay::generation_history::list("image").unwrap();
    assert_eq!(
        history.len(),
        1,
        "An unavailable parent discarded saved history"
    );
    assert_eq!(history[0].output_path, fixtures[ready].record.output_path);
    std::fs::rename(&fixtures[ready].root, &ready_original).unwrap();
    fixtures[ready].root = ready_original;
    std::fs::rename(&fixtures[offline].root, &original).unwrap();
    fixtures[offline].root = original;
    assert!(reconcile_product("image").unwrap().is_empty());
    assert!(load_store(&path).unwrap().entries.is_empty());
    assert!(load_store(&path).unwrap().cancellations.is_empty());
    assert!(!Path::new(&fixtures[offline].record.output_path).exists());
    assert!(
        crate::overlay::creation_intent_journal::load("image")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn unavailable_destination_preserves_independent_admission_and_cancellation() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../parity-fixtures/creation/destination-availability.json"
    ))
    .unwrap();
    if let Ok(index) = std::env::var(CASE_ENV) {
        exercise(&fixture["reconciliationCases"][index.parse::<usize>().unwrap()]);
        return;
    }
    for index in 0..fixture["reconciliationCases"].as_array().unwrap().len() {
        let root = root("destination-native");
        std::fs::create_dir(&root).unwrap();
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture"])
            .env(crate::paths::RUNTIME_STATE_ROOT_ENV, &root)
            .env(CASE_ENV, index.to_string())
            .stdin(Stdio::null())
            .output()
            .unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        assert!(
            output.status.success(),
            "{} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
