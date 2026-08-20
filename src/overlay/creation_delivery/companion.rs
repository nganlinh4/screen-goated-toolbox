use std::io::{Read as _, Seek as _, Write as _};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{DeliveryRecord, PublishedDelivery};

#[derive(Clone)]
pub struct PublishedCompanion {
    pub output_name: String,
    pub staging_path: String,
    pub output_path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CompanionRecord {
    pub output_name: String,
    pub staging_path: String,
    pub output_path: String,
    pub artifact_size_bytes: u64,
    pub artifact_sha256: String,
    #[serde(default)]
    pub file_identity: Option<String>,
}

pub(super) fn inspect(
    delivery: &PublishedDelivery,
) -> Result<
    Option<(
        PublishedCompanion,
        crate::overlay::generation_history::DeliveryArtifactIdentity,
    )>,
    String,
> {
    let Some(companion) = delivery.companion.clone() else {
        return Ok(None);
    };
    validate_assignment(delivery, &companion)?;
    let artifact =
        crate::overlay::generation_history::inspect_delivery_artifact(&companion.staging_path)?;
    Ok(Some((companion, artifact)))
}

pub(super) fn saved(
    companion: PublishedCompanion,
    artifact: crate::overlay::generation_history::DeliveryArtifactIdentity,
) -> CompanionRecord {
    CompanionRecord {
        output_name: companion.output_name,
        staging_path: companion.staging_path,
        output_path: companion.output_path,
        artifact_size_bytes: artifact.size_bytes,
        artifact_sha256: artifact.sha256,
        file_identity: None,
    }
}

pub(super) fn validate_saved(record: &CompanionRecord) -> Result<String, String> {
    if record.artifact_size_bytes == 0
        || record.artifact_sha256.len() != 64
        || !record
            .artifact_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || record
            .file_identity
            .as_deref()
            .is_some_and(|identity| !crate::overlay::creation_file_identity::valid(identity))
    {
        return Err("Creation companion state is invalid.".to_string());
    }
    super::publication::output_identity(Path::new(&record.output_path))
}

fn validate_assignment(
    delivery: &PublishedDelivery,
    companion: &PublishedCompanion,
) -> Result<(), String> {
    let staging = Path::new(&companion.staging_path);
    let output = Path::new(&companion.output_path);
    let primary_staging = Path::new(&delivery.staging_path);
    let primary_output = Path::new(&delivery.output_path);
    let valid_name =
        output.file_name().and_then(|value| value.to_str()) == Some(companion.output_name.as_str());
    let same_staging_parent = staging.parent() == primary_staging.parent();
    let same_output_parent = output.parent() == primary_output.parent();
    let matching_stem = staging.file_stem() == primary_staging.file_stem()
        && output.file_stem() == primary_output.file_stem();
    let fbx = output
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("fbx"));
    if !valid_name || !same_staging_parent || !same_output_parent || !matching_stem || !fbx {
        return Err("Creation companion assignment is invalid.".to_string());
    }
    let metadata = std::fs::symlink_metadata(staging)
        .map_err(|_| "Creation companion staging result is unavailable.".to_string())?;
    if !metadata.file_type().is_file() || metadata.len() == 0 {
        return Err("Creation companion staging result is invalid.".to_string());
    }
    let mut header = [0_u8; 21];
    std::fs::File::open(staging)
        .and_then(|mut file| file.read_exact(&mut header))
        .map_err(|_| "Creation companion staging result is invalid.".to_string())?;
    if &header != b"Kaydara FBX Binary  \0" {
        return Err("Creation companion staging result is invalid.".to_string());
    }
    super::publication::output_identity(output)?;
    Ok(())
}

pub(super) fn reserve(record: &CompanionRecord) -> Result<String, String> {
    let path = Path::new(&record.output_path);
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "Creation companion destination is already in use.".to_string())?;
    crate::overlay::creation_file_identity::from_file(&file)
}

pub(super) fn publish(record: &CompanionRecord) -> Result<(), String> {
    let identity = record
        .file_identity
        .as_deref()
        .ok_or_else(|| "Creation companion ownership is missing.".to_string())?;
    let path = Path::new(&record.output_path);
    let mut target = super::publication::lock_owned_path(path, Some(identity), true, true)?;
    let mut source = std::fs::File::open(&record.staging_path)
        .map_err(|_| "Creation companion staging result is unavailable.".to_string())?;
    target
        .set_len(0)
        .and_then(|()| target.rewind())
        .and_then(|()| std::io::copy(&mut source, &mut target).map(|_| ()))
        .and_then(|()| target.flush())
        .and_then(|()| target.sync_all())
        .map_err(|_| "Creation companion could not be published.".to_string())?;
    let artifact =
        crate::overlay::generation_history::inspect_delivery_artifact(&record.output_path)?;
    if artifact.size_bytes != record.artifact_size_bytes
        || artifact.sha256 != record.artifact_sha256
        || artifact.file_identity != identity
    {
        return Err("Creation companion changed during publication.".to_string());
    }
    Ok(())
}

pub(super) fn cancel(record: &CompanionRecord) -> Result<(), String> {
    let Some(identity) = record.file_identity.as_deref() else {
        return Ok(());
    };
    let path = Path::new(&record.output_path);
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Creation companion destination is unavailable.".to_string()),
        Ok(_) => {
            let file = super::publication::lock_owned_path(path, Some(identity), false, true)?;
            super::publication::delete_owned(&file, path)
        }
    }
}

pub(super) fn cleanup_staging(record: &CompanionRecord) -> Result<(), String> {
    let path = Path::new(&record.staging_path);
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => std::fs::remove_file(path)
            .map_err(|_| "Creation companion staging cleanup could not finish.".to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => Err("Creation companion staging cleanup refused an unsafe entry.".to_string()),
    }
}

pub(super) fn metadata(record: &DeliveryRecord) -> Value {
    let mut metadata = record.metadata.clone();
    if let (Some(companion), Some(object)) = (&record.companion, metadata.as_object_mut()) {
        object.insert(
            "download".to_string(),
            json!({
                "path": companion.output_path,
                "name": companion.output_name,
                "sizeBytes": companion.artifact_size_bytes,
                "sha256": companion.artifact_sha256,
                "fileIdentity": companion.file_identity,
            }),
        );
    }
    metadata
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn companion_publication_keeps_the_quad_download_under_owned_identity() {
        let root = std::env::temp_dir().join(format!(
            "sgt-delivery-companion-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let staging = root.join("staging");
        let output = root.join("output");
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::create_dir_all(&output).unwrap();
        let primary_staging = staging.join("quad.glb");
        let companion_staging = staging.join("quad.fbx");
        let primary_output = output.join("quad.glb");
        let companion_output = output.join("quad.fbx");
        let mut bytes = b"Kaydara FBX Binary  \0".to_vec();
        bytes.extend_from_slice(b"quad-data");
        std::fs::write(&companion_staging, &bytes).unwrap();
        let delivery = PublishedDelivery {
            product: "3d",
            job_id: "job".to_string(),
            dispatch_id: "dispatch".to_string(),
            request_fingerprint: "a".repeat(64),
            source_path: String::new(),
            output_name: "quad.glb".to_string(),
            staging_path: primary_staging.to_string_lossy().to_string(),
            output_path: primary_output.to_string_lossy().to_string(),
            companion: Some(PublishedCompanion {
                output_name: "quad.fbx".to_string(),
                staging_path: companion_staging.to_string_lossy().to_string(),
                output_path: companion_output.to_string_lossy().to_string(),
            }),
            metadata: json!({}),
        };
        let (companion, artifact) = inspect(&delivery).unwrap().unwrap();
        let mut record = saved(companion, artifact);
        record.file_identity = Some(reserve(&record).unwrap());
        publish(&record).unwrap();
        assert_eq!(std::fs::read(&companion_output).unwrap(), bytes);
        cancel(&record).unwrap();
        assert!(!companion_output.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
