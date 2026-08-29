use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use flate2::read::GzDecoder;
use serde::Deserialize;
use sha2::{Digest, Sha256};

const DELIVERY_RAW: &str = include_str!(concat!(env!("OUT_DIR"), "/help_index_delivery.json"));
const FORMAT: &str = "json-gzip";
const MAX_COMPRESSED_BYTES: u64 = 4 * 1024 * 1024;
const MAX_EXPANDED_BYTES: u64 = 4 * 1024 * 1024;
const MAX_ENTRIES: usize = 128;
const PRODUCTION_PREFIX: &str =
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-bundles/";
const STAGING_PREFIX: &str =
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-staging/";

static MEMORY_CACHE: LazyLock<Mutex<Option<Vec<ChunkEntry>>>> = LazyLock::new(|| Mutex::new(None));

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct ChunkEntry {
    pub(super) path: String,
    pub(super) text: String,
    platforms: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeliveryRoot {
    schema_version: u32,
    version: String,
    help_index: Delivery,
}

#[derive(Clone, Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Delivery {
    id: String,
    asset: String,
    download_url: String,
    format: String,
    size_bytes: u64,
    sha256: String,
    expanded_size_bytes: u64,
    expanded_sha256: String,
    entry_count: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HelpIndex {
    schema_version: u32,
    entries: Vec<ChunkEntry>,
}

pub(super) fn get_help_index() -> Result<Vec<ChunkEntry>, String> {
    if let Some(entries) = MEMORY_CACHE.lock().unwrap().as_ref() {
        return Ok(entries.clone());
    }
    let delivery = parse_delivery(DELIVERY_RAW)?;
    let result = load_selected(&delivery).or_else(|download_error| {
        load_last_good().map_err(|cache_error| {
            format!("Failed to fetch help data: {download_error}; cached copy: {cache_error}")
        })
    })?;
    *MEMORY_CACHE.lock().unwrap() = Some(result.clone());
    Ok(result)
}

fn load_selected(delivery: &Delivery) -> Result<Vec<ChunkEntry>, String> {
    let root = cache_root();
    let asset_path = root.join(&delivery.asset);
    if fs::metadata(&asset_path).is_ok_and(|metadata| metadata.len() == delivery.size_bytes)
        && let Ok(bytes) = fs::read(&asset_path)
        && let Ok(entries) = verify_and_parse(delivery, &bytes)
    {
        return Ok(entries);
    }
    let bytes = download(delivery)?;
    let entries = verify_and_parse(delivery, &bytes)?;
    fs::create_dir_all(&root).map_err(|error| format!("Help cache unavailable: {error}"))?;
    atomic_write(&asset_path, &bytes)?;
    let expanded = expand(delivery, &bytes)?;
    atomic_write(&root.join("last-good.json"), &expanded)?;
    atomic_write(
        &root.join("last-good.sha256"),
        delivery.expanded_sha256.as_bytes(),
    )?;
    Ok(entries)
}

fn load_last_good() -> Result<Vec<ChunkEntry>, String> {
    let root = cache_root();
    let path = root.join("last-good.json");
    let size = fs::metadata(&path)
        .map_err(|error| format!("Help cache unavailable: {error}"))?
        .len();
    if size == 0 || size > MAX_EXPANDED_BYTES {
        return Err("Help cache size is invalid".to_string());
    }
    let bytes = fs::read(path).map_err(|error| format!("Help cache unavailable: {error}"))?;
    let expected = fs::read_to_string(root.join("last-good.sha256"))
        .map_err(|error| format!("Help cache identity unavailable: {error}"))?;
    if !valid_sha(&expected) || digest(&bytes) != expected {
        return Err("Help cache identity is invalid".to_string());
    }
    parse_index(&bytes, None)
}

fn parse_delivery(raw: &str) -> Result<Delivery, String> {
    let root: DeliveryRoot =
        serde_json::from_str(raw).map_err(|error| format!("Invalid help delivery: {error}"))?;
    let delivery = root.help_index;
    let prefix = if cfg!(sgt_staging_delivery) && delivery.download_url.starts_with(STAGING_PREFIX)
    {
        STAGING_PREFIX
    } else {
        PRODUCTION_PREFIX
    };
    let identities_are_valid = valid_sha(&delivery.sha256) && valid_sha(&delivery.expanded_sha256);
    let valid = root.schema_version == 1
        && !root.version.is_empty()
        && delivery.id == "help-index"
        && delivery.format == FORMAT
        && identities_are_valid
        && delivery.asset
            == format!(
                "help-index-v{}-{}.json.gz",
                root.version,
                &delivery.sha256[..16]
            )
        && delivery.download_url == format!("{prefix}{}", delivery.asset)
        && (1..=MAX_COMPRESSED_BYTES).contains(&delivery.size_bytes)
        && (1..=MAX_EXPANDED_BYTES).contains(&delivery.expanded_size_bytes)
        && (1..=MAX_ENTRIES).contains(&delivery.entry_count);
    valid
        .then_some(delivery)
        .ok_or_else(|| "Invalid help delivery contract".to_string())
}

fn download(delivery: &Delivery) -> Result<Vec<u8>, String> {
    let response = crate::api::client::UREQ_DOWNLOAD_AGENT
        .get(&delivery.download_url)
        .header("User-Agent", "ScreenGoatedToolbox-HelpData")
        .call()
        .map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(delivery.size_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 != delivery.size_bytes {
        return Err("Help data response length is invalid".to_string());
    }
    Ok(bytes)
}

fn verify_and_parse(delivery: &Delivery, bytes: &[u8]) -> Result<Vec<ChunkEntry>, String> {
    if bytes.len() as u64 != delivery.size_bytes || digest(bytes) != delivery.sha256 {
        return Err("Help data compressed identity is invalid".to_string());
    }
    let expanded = expand(delivery, bytes)?;
    parse_index(&expanded, Some(delivery.entry_count))
}

fn expand(delivery: &Delivery, bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut expanded = Vec::new();
    GzDecoder::new(bytes)
        .take(delivery.expanded_size_bytes + 1)
        .read_to_end(&mut expanded)
        .map_err(|error| format!("Help data could not be expanded: {error}"))?;
    if expanded.len() as u64 != delivery.expanded_size_bytes
        || digest(&expanded) != delivery.expanded_sha256
    {
        return Err("Help data expanded identity is invalid".to_string());
    }
    Ok(expanded)
}

fn parse_index(bytes: &[u8], expected_entries: Option<usize>) -> Result<Vec<ChunkEntry>, String> {
    if bytes.len() as u64 > MAX_EXPANDED_BYTES {
        return Err("Help data exceeds its size boundary".to_string());
    }
    let root: HelpIndex =
        serde_json::from_slice(bytes).map_err(|error| format!("Invalid help data: {error}"))?;
    let valid = root.schema_version == 1
        && !root.entries.is_empty()
        && root.entries.len() <= MAX_ENTRIES
        && expected_entries.is_none_or(|count| count == root.entries.len())
        && root.entries.iter().all(valid_entry);
    if !valid {
        return Err("Invalid help data content".to_string());
    }
    Ok(root
        .entries
        .into_iter()
        .filter(|entry| entry.platforms.iter().any(|platform| platform == "windows"))
        .collect())
}

fn valid_entry(entry: &ChunkEntry) -> bool {
    !entry.path.is_empty()
        && entry.path.len() <= 256
        && !entry.text.is_empty()
        && entry.text.len() <= 128 * 1024
        && !entry.platforms.is_empty()
        && entry
            .platforms
            .iter()
            .all(|value| value == "windows" || value == "android")
}

fn cache_root() -> PathBuf {
    crate::paths::app_runtime_local_data_dir().join("help-assistant")
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = path.with_extension("partial");
    fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
    if !path.exists() {
        return fs::rename(&temporary, path).map_err(|error| error.to_string());
    }
    let backup = path.with_extension("previous");
    if backup.exists() {
        fs::remove_file(&backup).map_err(|error| error.to_string())?;
    }
    fs::rename(path, &backup).map_err(|error| error.to_string())?;
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::rename(&backup, path);
        return Err(error.to_string());
    }
    let _ = fs::remove_file(backup);
    Ok(())
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn valid_sha(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_delivery_is_content_addressed() {
        let delivery = parse_delivery(DELIVERY_RAW).unwrap();
        assert!(delivery.asset.contains(&delivery.sha256[..16]));
        assert_eq!(delivery.format, FORMAT);
    }

    #[test]
    fn platform_filter_keeps_windows_and_shared_entries() {
        let raw = br#"{"schemaVersion":1,"entries":[{"path":"shared","text":"one","platforms":["windows","android"]},{"path":"android","text":"two","platforms":["android"]}]}"#;
        let entries = parse_index(raw, Some(2)).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "shared");
    }

    #[cfg(sgt_staging_delivery)]
    #[test]
    fn selected_remote_asset_downloads_and_verifies() {
        let delivery = parse_delivery(DELIVERY_RAW).unwrap();
        assert!(
            delivery.download_url.starts_with(STAGING_PREFIX)
                || delivery.download_url.starts_with(PRODUCTION_PREFIX)
        );
        let bytes = download(&delivery).unwrap();
        let entries = verify_and_parse(&delivery, &bytes).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(
            entries
                .iter()
                .all(|entry| entry.platforms.contains(&"windows".to_string()))
        );
    }
}
