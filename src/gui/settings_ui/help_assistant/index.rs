use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use base64::Engine;
use flate2::read::GzDecoder;
use serde::Deserialize;
use sha2::{Digest, Sha256};

const DELIVERY_RAW: &str = include_str!(concat!(env!("OUT_DIR"), "/help_index_delivery.json"));
const FORMAT: &str = "json-gzip";
const SOURCE_FORMAT: &str = "jsonl-gzip";
const LEGACY_BYTES: u64 = 4 << 20;
const SOURCE_COMPRESSED_BYTES: u64 = 64 << 20;
const SOURCE_EXPANDED_BYTES: u64 = 128 << 20;
const SOURCE_ENTRIES: usize = 131_072;
pub(super) const SOURCE_DIMENSIONS: usize = 768;
const PRODUCTION_PREFIX: &str =
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-bundles/";
const STAGING_PREFIX: &str =
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-staging/";

static MEMORY_CACHE: LazyLock<Mutex<Option<Arc<[ChunkEntry]>>>> =
    LazyLock::new(|| Mutex::new(None));

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct ChunkEntry {
    pub(super) path: String,
    pub(super) text: String,
    platforms: Vec<String>,
    #[serde(skip)]
    pub(super) line: Option<u32>,
    #[serde(skip)]
    pub(super) vector: Vec<i8>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceIndexHeader {
    schema_version: u32,
    embedding_model: String,
    dimensions: usize,
    source_revision: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceIndexEntry {
    #[serde(flatten)]
    chunk: ChunkEntry,
    start_line: u32,
    vector: String,
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

pub(super) fn get_help_index() -> Result<Arc<[ChunkEntry]>, String> {
    if let Some(entries) = MEMORY_CACHE.lock().unwrap().as_ref() {
        return Ok(entries.clone());
    }
    let delivery = parse_delivery(DELIVERY_RAW)?;
    let result: Arc<[_]> = load_selected(&delivery)
        .or_else(|download_error| {
            load_last_good().map_err(|cache_error| {
                format!("Failed to fetch help data: {download_error}; cached copy: {cache_error}")
            })
        })?
        .into();
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
    verify_compressed(delivery, &bytes)?;
    let expanded = expand(delivery, &bytes)?;
    let entries = parse_delivered(delivery, &expanded)?;
    fs::create_dir_all(&root).map_err(|error| format!("Help cache unavailable: {error}"))?;
    atomic_write(&asset_path, &bytes)?;
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
    if size == 0 || size > SOURCE_EXPANDED_BYTES {
        return Err("Help cache size is invalid".to_string());
    }
    let bytes = fs::read(path).map_err(|error| format!("Help cache unavailable: {error}"))?;
    let expected = fs::read_to_string(root.join("last-good.sha256"))
        .map_err(|error| format!("Help cache identity unavailable: {error}"))?;
    if !valid_sha(&expected) || digest(&bytes) != expected {
        return Err("Help cache identity is invalid".to_string());
    }
    if bytes.starts_with(b"{\"schemaVersion\":2,") {
        parse_source_index(&bytes, None)
    } else {
        parse_index(&bytes, None)
    }
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
    let (format, compressed_limit, expanded_limit, entries) = match root.schema_version {
        1 => (FORMAT, LEGACY_BYTES, LEGACY_BYTES, 128),
        2 => (
            SOURCE_FORMAT,
            SOURCE_COMPRESSED_BYTES,
            SOURCE_EXPANDED_BYTES,
            SOURCE_ENTRIES,
        ),
        _ => return Err("Unsupported help delivery schema".to_string()),
    };
    let valid = !root.version.is_empty()
        && delivery.id == "help-index"
        && delivery.format == format
        && identities_are_valid
        && delivery.asset
            == format!(
                "help-index-v{}-{}.json.gz",
                root.version,
                &delivery.sha256[..16]
            )
        && delivery.download_url == format!("{prefix}{}", delivery.asset)
        && (1..=compressed_limit).contains(&delivery.size_bytes)
        && (1..=expanded_limit).contains(&delivery.expanded_size_bytes)
        && (1..=entries).contains(&delivery.entry_count);
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
    verify_compressed(delivery, bytes)?;
    parse_delivered(delivery, &expand(delivery, bytes)?)
}

fn verify_compressed(delivery: &Delivery, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() as u64 != delivery.size_bytes || digest(bytes) != delivery.sha256 {
        return Err("Help data compressed identity is invalid".to_string());
    }
    Ok(())
}

fn parse_delivered(delivery: &Delivery, bytes: &[u8]) -> Result<Vec<ChunkEntry>, String> {
    if delivery.format == SOURCE_FORMAT {
        parse_source_index(bytes, Some(delivery.entry_count))
    } else {
        parse_index(bytes, Some(delivery.entry_count))
    }
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
    if bytes.len() as u64 > LEGACY_BYTES {
        return Err("Help data exceeds its size boundary".to_string());
    }
    let root: HelpIndex =
        serde_json::from_slice(bytes).map_err(|error| format!("Invalid help data: {error}"))?;
    let valid = root.schema_version == 1
        && !root.entries.is_empty()
        && root.entries.len() <= 128
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

fn parse_source_index(bytes: &[u8], expected: Option<usize>) -> Result<Vec<ChunkEntry>, String> {
    let mut lines = std::str::from_utf8(bytes)
        .map_err(|_| "Invalid help data encoding".to_string())?
        .lines();
    let invalid = || "Invalid help source index".to_string();
    let header: SourceIndexHeader =
        serde_json::from_str(lines.next().ok_or_else(invalid)?).map_err(|_| invalid())?;
    if header.schema_version != 2
        || header.dimensions != SOURCE_DIMENSIONS
        || header.embedding_model != crate::model_config::GEMINI_EMBEDDING_API_MODEL
        || header.source_revision.len() != 40
        || !valid_revision(&header.source_revision)
    {
        return Err(invalid());
    }
    let mut entries = Vec::new();
    let mut count = 0;
    for line in lines {
        count += 1;
        if count > SOURCE_ENTRIES || line.len() > 32 * 1024 {
            return Err(invalid());
        }
        let entry: SourceIndexEntry = serde_json::from_str(line).map_err(|_| invalid())?;
        if !valid_entry(&entry.chunk) || entry.start_line == 0 {
            return Err(invalid());
        }
        let vector = base64::engine::general_purpose::STANDARD
            .decode(entry.vector)
            .map_err(|_| invalid())?;
        if vector.len() != SOURCE_DIMENSIONS || vector.iter().all(|value| *value == 0) {
            return Err(invalid());
        }
        if entry
            .chunk
            .platforms
            .iter()
            .any(|platform| platform == "windows")
        {
            entries.push(ChunkEntry {
                line: Some(entry.start_line),
                vector: vector.into_iter().map(|value| value as i8).collect(),
                ..entry.chunk
            });
        }
    }
    if count == 0 || expected.is_some_and(|expected| count != expected) {
        return Err(invalid());
    }
    Ok(entries)
}

fn valid_revision(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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

/// Concurrent help questions can download together; the shared writer gives
/// each write a unique temporary sibling and replaces the target in one rename.
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    crate::atomic_json::write_bytes_atomic(path, bytes).map_err(|error| error.to_string())
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
        assert!([FORMAT, SOURCE_FORMAT].contains(&delivery.format.as_str()));
    }

    #[test]
    fn platform_filter_keeps_windows_and_shared_entries() {
        let raw = br#"{"schemaVersion":1,"entries":[{"path":"shared","text":"one","platforms":["windows","android"]},{"path":"android","text":"two","platforms":["android"]}]}"#;
        let entries = parse_index(raw, Some(2)).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "shared");
    }

    #[test]
    fn source_index_filters_platforms_and_rejects_an_incomplete_package() {
        use serde_json::json;

        let header = json!({
            "schemaVersion": 2,
            "embeddingModel": crate::model_config::GEMINI_EMBEDDING_API_MODEL,
            "sourceRevision": "a".repeat(40),
            "dimensions": SOURCE_DIMENSIONS,
        });
        let vector = base64::engine::general_purpose::STANDARD.encode([1_u8; SOURCE_DIMENSIONS]);
        let windows = json!({
            "path": "src/feature.rs", "text": "feature", "startLine": 5,
            "platforms": ["windows"], "vector": &vector,
        });
        let android = json!({
            "path": "mobile/feature.kt", "text": "feature", "startLine": 7,
            "platforms": ["android"], "vector": &vector,
        });
        let input = format!("{header}\n{windows}\n{android}\n");
        let entries = parse_source_index(input.as_bytes(), Some(2)).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].line, Some(5));
        assert_eq!(entries[0].vector.len(), SOURCE_DIMENSIONS);
        assert!(parse_source_index(input.as_bytes(), Some(3)).is_err());
    }

    #[test]
    #[ignore = "requires a local source package via SGT_HELP_TEST_MANIFEST"]
    fn verifies_the_generated_source_candidate() {
        let path = PathBuf::from(std::env::var_os("SGT_HELP_TEST_MANIFEST").unwrap());
        let delivery = parse_delivery(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(delivery.format, SOURCE_FORMAT);
        let bytes = fs::read(path.with_file_name(&delivery.asset)).unwrap();
        let entries = verify_and_parse(&delivery, &bytes).unwrap();
        assert!(entries.len() > 128);
    }

    #[test]
    fn concurrent_cache_writes_keep_each_file_whole() {
        let root = std::env::temp_dir().join(format!(
            "sgt-help-cache-{}-{}",
            std::process::id(),
            std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let index = root.join("last-good.json");
        let identity = root.join("last-good.sha256");
        let barrier = std::sync::Barrier::new(4);
        std::thread::scope(|scope| {
            for writer in 0..4 {
                let (path, bytes) = if writer % 2 == 0 {
                    (&index, b"{\"schemaVersion\":1}".as_slice())
                } else {
                    (&identity, [b'a'; 64].as_slice())
                };
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    for _ in 0..64 {
                        atomic_write(path, bytes).unwrap();
                    }
                });
            }
        });
        assert_eq!(fs::read(&index).unwrap(), b"{\"schemaVersion\":1}");
        assert_eq!(fs::read(&identity).unwrap(), [b'a'; 64]);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
        fs::remove_dir_all(root).unwrap();
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
        assert!(!entries.is_empty());
        assert!(
            entries
                .iter()
                .all(|entry| entry.platforms.contains(&"windows".to_string()))
        );
    }
}
