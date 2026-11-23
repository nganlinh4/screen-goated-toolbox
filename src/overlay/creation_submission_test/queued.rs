use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub(crate) struct SavedRequest {
    pub job_id: String,
    pub destination: PathBuf,
}

pub(crate) fn seed(
    root: &Path,
    product: &str,
    extension: &str,
    reference: bool,
    mut settings: Value,
    freeze: impl FnOnce(Value) -> Value,
) -> SavedRequest {
    let job_id = crate::overlay::creation_identity::random_id(&format!("{product}_")).unwrap();
    let dispatch =
        crate::overlay::creation_identity::random_id(&format!("{product}-dispatch-")).unwrap();
    let destination = root.join(crate::overlay::creation_identity::random_id("output-").unwrap());
    std::fs::create_dir(&destination).unwrap();
    let destination = std::fs::canonicalize(destination).unwrap();
    let output_name = format!("result-{dispatch}.{extension}");
    let staging =
        crate::overlay::creation_output::prepare_staging(&dispatch, &output_name).unwrap();
    let snapshot = reference.then(|| {
        let source = root.join(format!("source-{dispatch}.png"));
        image::RgbaImage::new(2, 2).save(&source).unwrap();
        let inspected = crate::overlay::creation_source::inspect_image(&source).unwrap();
        crate::overlay::creation_source_snapshot::prepare(product, &dispatch, &[inspected]).unwrap()
    });
    let paths = snapshot
        .as_ref()
        .map(|value| value.paths())
        .unwrap_or_default();
    settings["imagePath"] = paths.first().map_or(Value::Null, |path| json!(path));
    settings["imagePaths"] = json!(paths);
    settings["sourceDescriptors"] = snapshot
        .as_ref()
        .map_or_else(|| json!([]), |value| json!(value.descriptors()));
    settings["outputDir"] = json!(staging.directory());
    settings["finalOutputDir"] = json!(destination);
    settings["outputName"] = json!(output_name);
    let arguments = freeze(settings);
    crate::overlay::creation_intent_journal::record(product, &job_id, &dispatch, arguments)
        .unwrap();
    if let Some(snapshot) = snapshot {
        snapshot.persist();
    }
    staging.persist();
    SavedRequest {
        job_id,
        destination,
    }
}

pub(crate) fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../parity-fixtures/creation/queued-recovery.json"
    ))
    .unwrap()
}

pub(crate) fn journal_bytes() -> Vec<u8> {
    std::fs::read(crate::paths::app_runtime_local_data_dir().join("active-creation-intents.json"))
        .unwrap()
}

pub(crate) fn move_destination(saved: &SavedRequest) -> PathBuf {
    let moved = saved
        .destination
        .with_file_name(crate::overlay::creation_identity::random_id("unavailable-").unwrap());
    assert_eq!(moved.parent(), saved.destination.parent());
    assert!(!moved.exists());
    std::fs::rename(&saved.destination, &moved).unwrap();
    moved
}

pub(crate) fn expire_requests() {
    let path = crate::paths::app_runtime_local_data_dir().join("active-creation-intents.json");
    let mut journal: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let elapsed = crate::overlay::creation_process_supervisor::MAX_WALL_TIME_MS + 100;
    for entry in journal["entries"].as_array_mut().unwrap() {
        for key in ["createdAtMs", "acceptedAtMs", "deadlineAtMs", "expiresAtMs"] {
            entry[key] = json!(entry[key].as_u64().unwrap().checked_sub(elapsed).unwrap());
        }
    }
    crate::atomic_json::write_json_atomic(&path, &journal).unwrap();
}
