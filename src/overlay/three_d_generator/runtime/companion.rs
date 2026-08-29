use std::path::{Path, PathBuf};

use serde_json::Value;

use super::RuntimeOperation;

pub(super) struct ResultCompanion {
    pub delivery: crate::overlay::creation_delivery::PublishedCompanion,
    pub final_path: PathBuf,
    pub final_name: String,
    staging_path: PathBuf,
}

impl ResultCompanion {
    pub(super) fn cleanup_staging(&self) {
        let _ = std::fs::remove_file(&self.staging_path);
    }
}

pub(super) fn from_result(
    result: &Value,
    operation: &RuntimeOperation,
    primary_staging: &Path,
    primary_output: &Path,
) -> Result<Option<ResultCompanion>, String> {
    let Some(path) = result.get("downloadPath").and_then(Value::as_str) else {
        return Ok(None);
    };
    let staging_path = PathBuf::from(path);
    let expected_staging = primary_staging.with_extension("fbx");
    let final_path = primary_output.with_extension("fbx");
    let final_name = final_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "The model download name is invalid.".to_string())?
        .to_string();
    let reported_name = result.get("downloadName").and_then(Value::as_str);
    if !same_path(&staging_path, &expected_staging)
        || reported_name != expected_staging.file_name().and_then(|name| name.to_str())
        || expected_staging.parent() != Some(operation.output_dir())
    {
        return Err("The model download assignment is invalid.".to_string());
    }
    Ok(Some(ResultCompanion {
        delivery: crate::overlay::creation_delivery::PublishedCompanion {
            output_name: final_name.clone(),
            staging_path: staging_path.to_string_lossy().to_string(),
            output_path: final_path.to_string_lossy().to_string(),
        },
        final_path,
        final_name,
        staging_path,
    }))
}

fn same_path(left: &Path, right: &Path) -> bool {
    left.to_string_lossy()
        .eq_ignore_ascii_case(&right.to_string_lossy())
}
