use std::path::PathBuf;

use serde_json::Value;

use super::StartJobRequest;

fn expected_output_path(request: &StartJobRequest) -> Option<PathBuf> {
    let output_name = PathBuf::from(request.output_name.as_deref()?);
    if output_name.file_name() != Some(output_name.as_os_str()) {
        return None;
    }
    Some(PathBuf::from(request.output_dir.as_deref()?).join(output_name))
}

pub(super) fn cancelled_job_output(request: &StartJobRequest) -> Option<PathBuf> {
    let directory = PathBuf::from(request.output_dir.as_deref()?);
    let output = expected_output_path(request)?;
    let resolved_directory = std::fs::canonicalize(directory).ok()?;
    let resolved_output = std::fs::canonicalize(output).ok()?;
    (resolved_output.is_file() && resolved_output.parent() == Some(resolved_directory.as_path()))
        .then_some(resolved_output)
}

pub(super) fn validate_runtime_result(
    request: &StartJobRequest,
    value: Value,
) -> Result<Value, String> {
    let expected = expected_output_path(request)
        .ok_or_else(|| "Image output destination is invalid.".to_string())?;
    let reported = value
        .get("outputPath")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| "Creation engine returned no image path.".to_string())?;
    let expected = std::fs::canonicalize(expected)
        .map_err(|_| "Creation engine returned no image file.".to_string())?;
    let reported = std::fs::canonicalize(reported)
        .map_err(|_| "Creation engine returned no image file.".to_string())?;
    let metadata = std::fs::metadata(&expected)
        .map_err(|_| "Creation engine returned no image file.".to_string())?;
    if expected != reported || !metadata.is_file() || metadata.len() < 32 {
        return Err("Creation engine returned an invalid image file.".to_string());
    }
    if value.get("mimeType").and_then(Value::as_str) != Some("image/png")
        || value.get("width").and_then(Value::as_u64).unwrap_or(0) == 0
        || value.get("height").and_then(Value::as_u64).unwrap_or(0) == 0
    {
        return Err("Creation engine returned invalid image metadata.".to_string());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_directory(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "sgt-image-output-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[test]
    fn result_must_match_the_exact_assigned_file_and_metadata() {
        let directory = test_directory("exact");
        std::fs::create_dir(&directory).expect("create test directory");
        let expected = directory.join("Created Image 1.png");
        let conflicting = directory.join("other.png");
        std::fs::write(&expected, [7_u8; 64]).expect("write expected output");
        std::fs::write(&conflicting, [9_u8; 64]).expect("write conflicting output");
        let request = StartJobRequest {
            image_paths: vec!["reference.png".to_string()],
            image_path: Some("reference.png".to_string()),
            output_dir: Some(directory.to_string_lossy().to_string()),
            prompt: "transform".to_string(),
            output_name: Some("Created Image 1.png".to_string()),
        };
        let valid = json!({
            "outputPath": expected,
            "mimeType": "image/png",
            "width": 640,
            "height": 480,
        });
        assert!(validate_runtime_result(&request, valid).is_ok());
        let invalid = json!({
            "outputPath": conflicting,
            "mimeType": "image/png",
            "width": 640,
            "height": 480,
        });
        assert!(validate_runtime_result(&request, invalid).is_err());

        std::fs::remove_file(expected).expect("remove expected output");
        std::fs::remove_file(conflicting).expect("remove conflicting output");
        std::fs::remove_dir(directory).expect("remove test directory");
    }
}
