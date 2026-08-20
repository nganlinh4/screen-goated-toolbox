use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

fn root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "sgt-delivery-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

struct Fixture {
    root: PathBuf,
    record: DeliveryRecord,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let root = root(label);
        std::fs::create_dir_all(&root).unwrap();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dispatch_id = format!("image_dispatch_{nonce}");
        let output_name = format!("result-{dispatch_id}.png");
        let staging =
            crate::overlay::creation_output::prepare_staging(&dispatch_id, &output_name).unwrap();
        let staging_path =
            crate::overlay::creation_output::assigned_path(staging.directory(), &output_name)
                .unwrap();
        std::fs::write(&staging_path, b"published-image").unwrap();
        staging.persist();
        let artifact = crate::overlay::generation_history::inspect_delivery_artifact(
            &staging_path.to_string_lossy(),
        )
        .unwrap();
        let output_path = root.join(&output_name);
        let publication_path = publication::reserve_path(&output_path, &dispatch_id).unwrap();
        Self {
            root,
            record: DeliveryRecord {
                product: "image".to_string(),
                job_id: format!("image_job_{nonce}"),
                dispatch_id,
                request_fingerprint: "a".repeat(64),
                source_path: String::new(),
                output_name,
                staging_path: staging_path.to_string_lossy().to_string(),
                output_path: output_path.to_string_lossy().to_string(),
                publication_path: publication_path.to_string_lossy().to_string(),
                publication_claim: publication::new_claim().unwrap(),
                publication_file_identity: None,
                metadata: serde_json::json!({"operation": "create_image"}),
                artifact_size_bytes: artifact.size_bytes,
                artifact_sha256: artifact.sha256,
                companion: None,
                stage: DeliveryStage::Validated,
            },
        }
    }

    fn commit_receipt(&mut self) {
        let reservation = publication::create_receipt(&self.record).unwrap();
        self.record.publication_file_identity = Some(reservation.identity().to_string());
        reservation.commit().unwrap();
    }

    fn prepare(&mut self) {
        self.commit_receipt();
        publication::prepare(&self.record).unwrap();
        self.record.stage = DeliveryStage::PublicationPrepared;
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.record.publication_path);
        let _ = std::fs::remove_file(&self.record.output_path);
        let _ = crate::overlay::creation_output::cleanup_staging(
            &self.record.dispatch_id,
            &self.record.output_name,
            Path::new(&self.record.staging_path),
        );
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn receipt_created_before_identity_save_is_recovered_by_its_durable_claim() {
    let fixture = Fixture::new("reservation-drop");
    let reservation = publication::create_receipt(&fixture.record).unwrap();
    let identity = reservation.identity().to_string();
    assert!(Path::new(&fixture.record.publication_path).exists());
    drop(reservation);
    let recovered = publication::create_receipt(&fixture.record).unwrap();
    assert_eq!(recovered.identity(), identity);
    recovered.commit().unwrap();
}

#[test]
fn journaled_receipt_survives_guard_and_keeps_its_identity() {
    let mut fixture = Fixture::new("reservation-commit");
    fixture.commit_receipt();
    assert!(Path::new(&fixture.record.publication_path).is_file());
    publication::cleanup_receipt(&fixture.record).unwrap();
    assert!(!Path::new(&fixture.record.publication_path).exists());
}

#[test]
fn prepared_receipt_publishes_by_verified_handle_without_replacement() {
    let mut fixture = Fixture::new("handle-publish");
    fixture.prepare();
    let published = publication::publish_prepared(&fixture.record).unwrap();
    assert_eq!(
        published.artifact().file_identity,
        fixture.record.publication_file_identity.clone().unwrap()
    );
    assert!(!Path::new(&fixture.record.publication_path).exists());
    assert_eq!(
        std::fs::read(&fixture.record.output_path).unwrap(),
        b"published-image"
    );
}

#[test]
fn held_receipt_identity_blocks_path_swap_before_publish_and_delete() {
    let mut fixture = Fixture::new("handle-swap");
    fixture.prepare();
    let receipt = Path::new(&fixture.record.publication_path);
    let moved = fixture.root.join("foreign-slot.tmp");
    let owned = publication::lock_owned_path(
        receipt,
        fixture.record.publication_file_identity.as_deref(),
        false,
        true,
    )
    .unwrap();
    assert!(std::fs::rename(receipt, &moved).is_err());
    publication::delete_owned(&owned, receipt).unwrap();
    drop(owned);
    assert!(!receipt.exists());
    assert!(!moved.exists());
}

#[test]
fn prepared_cancellation_is_idempotent_after_receipt_delete_cutpoint() {
    let mut fixture = Fixture::new("cancel-cutpoint");
    fixture.prepare();
    publication::cancel_pre_publication(&fixture.record).unwrap();
    assert!(!Path::new(&fixture.record.publication_path).exists());
    assert!(!Path::new(&fixture.record.output_path).exists());
    publication::cancel_pre_publication(&fixture.record).unwrap();
}

#[test]
fn prepared_cancellation_deletes_only_the_exact_crash_moved_output() {
    let mut fixture = Fixture::new("cancel-moved");
    fixture.prepare();
    drop(publication::publish_prepared(&fixture.record).unwrap());
    publication::cancel_pre_publication(&fixture.record).unwrap();
    assert!(!Path::new(&fixture.record.output_path).exists());
}

#[test]
fn prepared_cancellation_rejects_identical_foreign_final_file() {
    let mut fixture = Fixture::new("cancel-foreign");
    fixture.prepare();
    drop(publication::publish_prepared(&fixture.record).unwrap());
    std::fs::remove_file(&fixture.record.output_path).unwrap();
    std::fs::write(&fixture.record.output_path, b"published-image").unwrap();
    assert!(publication::cancel_pre_publication(&fixture.record).is_err());
    assert_eq!(
        std::fs::read(&fixture.record.output_path).unwrap(),
        b"published-image"
    );
}

#[test]
fn published_verification_rejects_identical_same_path_replacement() {
    let mut fixture = Fixture::new("published-replacement");
    fixture.prepare();
    drop(publication::publish_prepared(&fixture.record).unwrap());
    fixture.record.stage = DeliveryStage::Published;
    std::fs::remove_file(&fixture.record.output_path).unwrap();
    std::fs::write(&fixture.record.output_path, b"published-image").unwrap();
    assert!(publication::verify_published(&fixture.record).is_err());
}

#[test]
fn corrupt_and_oversized_delivery_indexes_are_preserved() {
    let root = root("invalid-index");
    std::fs::create_dir_all(&root).unwrap();
    let journal = root.join("deliveries.json");
    let corrupt = b"{ definitely-not-valid-json";
    std::fs::write(&journal, corrupt).unwrap();
    assert!(load_store(&journal).is_err());
    assert_eq!(std::fs::read(&journal).unwrap(), corrupt);
    let file = std::fs::File::create(&journal).unwrap();
    file.set_len(MAX_FILE_BYTES + 1).unwrap();
    drop(file);
    assert!(load_store(&journal).is_err());
    assert_eq!(
        std::fs::symlink_metadata(&journal).unwrap().len(),
        MAX_FILE_BYTES + 1
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn duplicate_dispatch_job_and_output_identities_fail_closed() {
    let first = Fixture::new("duplicates-first");
    let second = Fixture::new("duplicates-second");
    let cases = [
        DeliveryRecord {
            dispatch_id: first.record.dispatch_id.clone(),
            ..second.record.clone()
        },
        DeliveryRecord {
            product: first.record.product.clone(),
            job_id: first.record.job_id.clone(),
            ..second.record.clone()
        },
        DeliveryRecord {
            output_name: first.record.output_name.clone(),
            output_path: first.record.output_path.clone(),
            ..second.record.clone()
        },
    ];
    for ambiguous in cases {
        let path = first.root.join(format!("{}.json", ambiguous.dispatch_id));
        let store = DeliveryStore {
            version: VERSION,
            entries: vec![first.record.clone(), ambiguous],
            cancellations: Vec::new(),
        };
        std::fs::write(&path, serde_json::to_vec(&store).unwrap()).unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(load_store(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}

#[test]
fn one_cancellation_record_fits_the_reserved_serialized_budget() {
    let now = now_ms();
    let cancellation = CancellationRecord {
        product: "image".to_string(),
        job_id: "j".repeat(MAX_ID_BYTES),
        dispatch_id: "d".repeat(MAX_ID_BYTES),
        request_fingerprint: "a".repeat(64),
        output_name: format!("{}.png", "o".repeat(240)),
        created_at_ms: now,
        expires_at_ms: now.saturating_add(CANCELLATION_LIFETIME_MS),
    };
    assert!(
        serde_json::to_vec(&cancellation).unwrap().len() as u64
            <= MAX_SERIALIZED_CANCELLATION_BYTES
    );
}

#[test]
fn cooldown_requests_start_with_a_delay_instead_of_losing_retry_ownership() {
    assert_eq!(retry_start_delay_ms(10_000, 35_000), 25_000);
    assert_eq!(retry_start_delay_ms(35_000, 10_000), 0);
}
