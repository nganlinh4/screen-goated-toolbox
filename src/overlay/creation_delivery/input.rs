use std::path::Path;

use super::*;

pub(super) fn validate_input(delivery: &PublishedDelivery) -> Result<(), String> {
    validate_product(delivery.product)?;
    if !valid_id(&delivery.job_id)
        || !valid_id(&delivery.dispatch_id)
        || delivery.source_path.len() > MAX_PATH_BYTES
        || delivery.staging_path.len() > MAX_PATH_BYTES
        || delivery.output_path.len() > MAX_PATH_BYTES
        || !Path::new(&delivery.staging_path).is_absolute()
        || !Path::new(&delivery.output_path).is_absolute()
        || !valid_fingerprint(&delivery.request_fingerprint)
        || serde_json::to_vec(&delivery.metadata)
            .map(|bytes| bytes.len() > MAX_METADATA_BYTES)
            .unwrap_or(true)
    {
        return Err("Creation delivery state is invalid.".to_string());
    }
    let output_name = Path::new(&delivery.output_path)
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Creation delivery state is invalid.".to_string())?;
    if output_name != delivery.output_name
        || !valid_product_output_name(delivery.product, output_name)
    {
        return Err("Creation delivery output assignment is invalid.".to_string());
    }
    publication::output_identity(Path::new(&delivery.output_path))?;
    crate::overlay::creation_output::validate_staging_path(
        &delivery.dispatch_id,
        output_name,
        Path::new(&delivery.staging_path),
    )?;
    Ok(())
}

pub(super) fn verify_active_intent(
    product: &str,
    job_id: &str,
    dispatch_id: &str,
    request_fingerprint: &str,
    output_name: &str,
    staging_path: &str,
    output_path: &str,
) -> Result<(), String> {
    let assignment = crate::overlay::creation_intent_journal::verify_delivery_assignment(
        product,
        job_id,
        dispatch_id,
        request_fingerprint,
        output_name,
    )?;
    if !same_path(&assignment.staging_path.to_string_lossy(), staging_path)
        || publication::output_identity(&assignment.output_path)?
            != publication::output_identity(Path::new(output_path))?
    {
        return Err("Creation delivery conflicts with its accepted request.".to_string());
    }
    Ok(())
}

pub(super) fn validate_cancellation(cancellation: &CancelledDelivery) -> Result<(), String> {
    validate_product(cancellation.product)?;
    if !valid_id(&cancellation.job_id)
        || !valid_id(&cancellation.dispatch_id)
        || !valid_fingerprint(&cancellation.request_fingerprint)
        || cancellation.output_name.is_empty()
        || cancellation.output_name.len() > 255
        || !valid_product_output_name(cancellation.product, &cancellation.output_name)
    {
        return Err("Creation cancellation state is invalid.".to_string());
    }
    crate::overlay::creation_output::assigned_path(Path::new("."), &cancellation.output_name)
        .map(|_| ())
}

pub(super) fn validate_saved_delivery(
    saved: &DeliveryRecord,
    delivery: &PublishedDelivery,
    artifact: &crate::overlay::generation_history::DeliveryArtifactIdentity,
) -> Result<(), String> {
    let companion = companion::inspect(delivery)?;
    let requested = companion.as_ref().map(|(value, artifact)| {
        (
            &value.output_name,
            &value.staging_path,
            &value.output_path,
            artifact.size_bytes,
            &artifact.sha256,
        )
    });
    let saved_companion = saved.companion.as_ref().map(|value| {
        (
            &value.output_name,
            &value.staging_path,
            &value.output_path,
            value.artifact_size_bytes,
            &value.artifact_sha256,
        )
    });
    if saved.product != delivery.product
        || saved.job_id != delivery.job_id
        || saved.request_fingerprint != delivery.request_fingerprint
        || saved.source_path != delivery.source_path
        || saved.output_name != delivery.output_name
        || !same_path(&saved.staging_path, &delivery.staging_path)
        || !same_path(&saved.output_path, &delivery.output_path)
        || saved.metadata != delivery.metadata
        || saved.artifact_size_bytes != artifact.size_bytes
        || saved.artifact_sha256 != artifact.sha256
        || saved_companion != requested
    {
        return Err("Creation delivery conflicts with its saved state.".to_string());
    }
    Ok(())
}

pub(super) fn validate_product(product: &str) -> Result<(), String> {
    matches!(product, "3d" | "svg" | "image")
        .then_some(())
        .ok_or_else(|| "Creation delivery product is invalid.".to_string())
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn valid_fingerprint(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_product_output_name(product: &str, output_name: &str) -> bool {
    matches!(
        (
            product,
            Path::new(output_name)
                .extension()
                .and_then(|value| value.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
        ),
        ("3d", Some("glb")) | ("svg", Some("svg")) | ("image", Some("png"))
    )
}

pub(super) fn same_path(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}
