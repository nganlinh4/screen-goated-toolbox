use crate::tensor::{Device, Tensor};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;

/// Load all tensors from a model directory.
///
/// Supports both single-file (`model.safetensors`) and sharded
/// (`model.safetensors.index.json` + `model-00001-of-N.safetensors`) formats.
pub fn load_model_weights(model_dir: &Path, device: Device) -> Result<HashMap<String, Tensor>> {
    let single_path = model_dir.join("model.safetensors");
    let index_path = model_dir.join("model.safetensors.index.json");

    if single_path.exists() {
        tracing::info!("Loading weights from {:?}", single_path);
        load_safetensors(&single_path, device)
    } else if index_path.exists() {
        tracing::info!("Loading sharded weights from {:?}", index_path);
        load_sharded_safetensors(&index_path, device)
    } else {
        anyhow::bail!(
            "No model weights found in {:?} (expected model.safetensors or model.safetensors.index.json)",
            model_dir
        )
    }
}

/// Load sharded safetensors using the index file.
fn load_sharded_safetensors(index_path: &Path, device: Device) -> Result<HashMap<String, Tensor>> {
    let index_data = std::fs::read_to_string(index_path)
        .with_context(|| format!("Failed to read index: {:?}", index_path))?;
    let index: serde_json::Value =
        serde_json::from_str(&index_data).with_context(|| "Failed to parse safetensors index")?;

    let weight_map = index["weight_map"]
        .as_object()
        .context("Missing weight_map in index")?;

    // Collect unique shard filenames
    let mut shard_files: Vec<String> = weight_map
        .values()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();
    shard_files.sort();
    shard_files.dedup();

    let model_dir = index_path.parent().unwrap();
    let mut all_weights = HashMap::new();

    for shard_file in &shard_files {
        let shard_path = model_dir.join(shard_file);
        tracing::info!("Loading shard: {}", shard_file);
        let shard_weights = load_safetensors(&shard_path, device)
            .with_context(|| format!("Failed to load shard: {}", shard_file))?;
        all_weights.extend(shard_weights);
    }

    Ok(all_weights)
}

/// Load all tensors from a single safetensors file.
#[cfg(feature = "tch-backend")]
pub fn load_safetensors(path: &Path, device: Device) -> Result<HashMap<String, Tensor>> {
    let tch_device = tch::Device::from(device);
    let tensors = tch::Tensor::read_safetensors(path)
        .with_context(|| format!("Failed to read safetensors: {:?}", path))?;

    Ok(tensors
        .into_iter()
        .map(|(name, tensor)| (name, Tensor::from_tch(tensor.to_device(tch_device))))
        .collect())
}

/// Load all tensors from a single safetensors file (MLX backend).
#[cfg(feature = "mlx")]
pub fn load_safetensors(path: &Path, _device: Device) -> Result<HashMap<String, Tensor>> {
    let map =
        crate::backend::mlx::io::load_safetensors(path).map_err(|e| anyhow::anyhow!("{}", e))?;
    Ok(map
        .into_iter()
        .map(|(name, arr)| (name, Tensor::from_mlx(arr)))
        .collect())
}

/// Get a tensor from the weights map with a given prefix and suffix.
pub fn get_weight(weights: &HashMap<String, Tensor>, prefix: &str, name: &str) -> Result<Tensor> {
    let key = if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{}.{}", prefix, name)
    };
    weights
        .get(&key)
        .map(|t| t.shallow_clone())
        .with_context(|| format!("Weight not found: {}", key))
}

/// Get an optional tensor (returns None if not found).
pub fn get_weight_opt(
    weights: &HashMap<String, Tensor>,
    prefix: &str,
    name: &str,
) -> Option<Tensor> {
    let key = if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{}.{}", prefix, name)
    };
    weights.get(&key).map(|t| t.shallow_clone())
}
