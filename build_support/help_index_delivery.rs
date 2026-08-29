use std::fs;
use std::path::Path;

use serde_json::Value;

const DEFAULT_MANIFEST: &str = "component-delivery/help-index-v1.json";
const MAXIMUM_BYTES: u64 = 4 * 1024 * 1024;

pub(crate) fn generate(manifest_dir: &Path, out_dir: &Path) {
    let selected = crate::delivery_channel::select(manifest_dir, DEFAULT_MANIFEST);
    let raw = fs::read_to_string(&selected.path).unwrap_or_else(|error| {
        panic!(
            "failed to read help-index delivery {}: {error}",
            selected.path.display()
        )
    });
    let root: Value = serde_json::from_str(&raw)
        .unwrap_or_else(|error| panic!("invalid help-index delivery: {error}"));
    assert_eq!(root["schemaVersion"].as_u64(), Some(1));
    let version = required_string(&root, "version");
    let delivery = root["helpIndex"]
        .as_object()
        .expect("help-index delivery has no helpIndex record");
    let asset = required_object_string(delivery, "asset");
    let url = required_object_string(delivery, "downloadUrl");
    let sha256 = required_object_string(delivery, "sha256");
    let expanded_sha256 = required_object_string(delivery, "expandedSha256");
    assert_eq!(required_object_string(delivery, "id"), "help-index");
    assert_eq!(required_object_string(delivery, "format"), "json-gzip");
    assert!(valid_sha(sha256) && valid_sha(expanded_sha256));
    assert_eq!(
        asset,
        format!("help-index-v{version}-{}.json.gz", &sha256[..16])
    );
    crate::delivery_channel::assert_candidate_asset_url(
        selected.channel,
        asset,
        url,
        "help-index asset",
    );
    for key in ["sizeBytes", "expandedSizeBytes"] {
        assert!(
            (1..=MAXIMUM_BYTES).contains(
                &delivery[key]
                    .as_u64()
                    .unwrap_or_else(|| panic!("help-index delivery has invalid {key}"))
            )
        );
    }
    assert!((1..=128).contains(&delivery["entryCount"].as_u64().unwrap_or_default()));
    fs::write(out_dir.join("help_index_delivery.json"), raw)
        .expect("failed to generate help-index delivery");
}

fn required_string<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key]
        .as_str()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| panic!("help-index delivery has invalid {key}"))
}

fn required_object_string<'a>(value: &'a serde_json::Map<String, Value>, key: &str) -> &'a str {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| panic!("help-index delivery has invalid {key}"))
}

fn valid_sha(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
