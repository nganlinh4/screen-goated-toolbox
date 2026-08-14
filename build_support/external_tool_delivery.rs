use std::collections::HashSet;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

const DEFAULT_MANIFEST: &str = "component-delivery/windows/external-tools-v1.json";
const IDS: &[&str] = &["yt-dlp-x64", "ffmpeg-x64", "deno-x64"];

struct SourceContract<'a> {
    id: &'a str,
    version: &'a str,
    asset: &'a str,
    url: &'a str,
    digest: &'a str,
    format: &'a str,
}

pub(crate) fn generate(manifest_dir: &Path, out_dir: &Path) {
    let selected = crate::delivery_channel::select(manifest_dir, DEFAULT_MANIFEST);
    let configured = selected.path;
    assert!(
        configured.is_file(),
        "missing verified external-tool delivery: {}",
        configured.display()
    );
    let generated = delivery_source(&configured, selected.channel);
    let output = out_dir.join("external_tool_delivery.rs");
    fs::write(&output, generated)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", output.display()));
}

fn delivery_source(path: &Path, channel: crate::delivery_channel::DeliveryChannel) -> String {
    let raw = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    let root: Value = serde_json::from_str(&raw)
        .unwrap_or_else(|error| panic!("invalid {}: {error}", path.display()));
    assert_eq!(
        field_u64(root.as_object().unwrap(), "schemaVersion", path),
        1
    );
    assert_eq!(
        field_str(root.as_object().unwrap(), "architecture", path),
        "x64"
    );
    assert_eq!(
        field_str(root.as_object().unwrap(), "hostVersion", path),
        env!("CARGO_PKG_VERSION"),
        "{} targets a different host version",
        path.display()
    );
    let components = root["components"]
        .as_array()
        .unwrap_or_else(|| panic!("{} is missing components", path.display()));
    assert_eq!(
        components.len(),
        IDS.len(),
        "{} must deliver three tools",
        path.display()
    );

    let mut seen = HashSet::new();
    let mut source = String::from("const EXTERNAL_TOOL_DELIVERIES: &[ExternalToolDelivery] = &[\n");
    for value in components {
        let component = value
            .as_object()
            .unwrap_or_else(|| panic!("{} has an invalid component", path.display()));
        let id = field_str(component, "id", path);
        assert!(
            IDS.contains(&id) && seen.insert(id),
            "{} has an invalid tool id",
            path.display()
        );
        let version = field_str(component, "version", path);
        identifier(version, path);
        let asset = field_str(component, "asset", path);
        let url = field_str(component, "downloadUrl", path);
        let archive_format = field_str(component, "archiveFormat", path);
        assert!(matches!(archive_format, "raw" | "zip"));
        let digest = field_str(component, "sha256", path);
        sha256(digest, path);
        validate_source(
            SourceContract {
                id,
                version,
                asset,
                url,
                digest,
                format: archive_format,
            },
            path,
            channel,
        );
        let size = positive_u64(component, "sizeBytes", path);
        let unpacked = positive_u64(component, "unpackedSizeBytes", path);
        let files = component["files"]
            .as_array()
            .unwrap_or_else(|| panic!("{} tool {id} has no files", path.display()));
        assert!(!files.is_empty() && files.len() <= 8);
        let expected = expected_paths(id);
        assert_eq!(files.len(), expected.len());
        let mut file_seen = HashSet::new();
        let mut file_total = 0_u64;
        let mut file_source = String::new();
        for file in files {
            let file = file.as_object().unwrap();
            let installed_path = field_str(file, "path", path);
            let archive_path = field_str(file, "archivePath", path);
            relative_path(installed_path, path);
            relative_path(archive_path, path);
            assert!(expected.contains(&installed_path) && file_seen.insert(installed_path));
            let file_size = positive_u64(file, "sizeBytes", path);
            file_total = file_total
                .checked_add(file_size)
                .expect("tool size overflow");
            let file_digest = field_str(file, "sha256", path);
            sha256(file_digest, path);
            file_source.push_str(&format!(
                "            ExternalToolFile {{ path: {installed_path:?}, archive_path: {archive_path:?}, size_bytes: {file_size}, sha256: {file_digest:?} }},\n"
            ));
        }
        assert_eq!(file_total, unpacked);
        source.push_str(&format!(
            "    ExternalToolDelivery {{ id: {id:?}, version: {version:?}, asset: {asset:?}, download_url: {url:?}, archive_format: ExternalArchiveFormat::{}, size_bytes: {size}, sha256: {digest:?}, unpacked_size_bytes: {unpacked}, files: &[\n{file_source}        ] }},\n",
            if archive_format == "raw" { "Raw" } else { "Zip" }
        ));
    }
    assert_eq!(seen.len(), IDS.len());
    source.push_str("];\n");
    source.push_str(&webview_source(&root, path, channel));
    source
}

fn validate_source(
    source: SourceContract<'_>,
    path: &Path,
    channel: crate::delivery_channel::DeliveryChannel,
) {
    let SourceContract {
        id,
        version,
        asset,
        url,
        digest,
        format,
    } = source;
    match id {
        "yt-dlp-x64" => {
            assert_eq!(format, "raw");
            assert_eq!(asset, "yt-dlp.exe");
            assert_eq!(
                url,
                format!("https://github.com/yt-dlp/yt-dlp/releases/download/{version}/yt-dlp.exe")
            );
        }
        "deno-x64" => {
            assert_eq!(format, "zip");
            assert_eq!(asset, "deno-x86_64-pc-windows-msvc.zip");
            assert_eq!(
                url,
                format!("https://github.com/denoland/deno/releases/download/v{version}/{asset}")
            );
        }
        "ffmpeg-x64" => {
            assert_eq!(format, "zip");
            assert_eq!(asset, format!("{id}-{version}-{}.zip", &digest[..16]));
            crate::delivery_channel::assert_owned_asset_url(channel, asset, url, "FFmpeg URL");
        }
        _ => panic!("{} has an unsupported tool", path.display()),
    }
}

fn expected_paths(id: &str) -> &'static [&'static str] {
    match id {
        "yt-dlp-x64" => &["bin/x64/yt-dlp.exe"],
        "deno-x64" => &["bin/x64/deno.exe"],
        "ffmpeg-x64" => &[
            "bin/x64/ffmpeg.exe",
            "bin/x64/ffprobe.exe",
            "licenses/LICENSE.txt",
            "licenses/SOURCE.txt",
        ],
        _ => unreachable!(),
    }
}

fn webview_source(
    root: &Value,
    path: &Path,
    channel: crate::delivery_channel::DeliveryChannel,
) -> String {
    let value = root["webview2Bootstrapper"]
        .as_object()
        .unwrap_or_else(|| panic!("{} is missing webview2Bootstrapper", path.display()));
    let version = field_str(value, "version", path);
    identifier(version, path);
    let digest = field_str(value, "sha256", path);
    sha256(digest, path);
    let asset = field_str(value, "asset", path);
    assert_eq!(
        asset,
        format!("webview2-bootstrapper-{version}-{}.exe", &digest[..16])
    );
    let url = field_str(value, "downloadUrl", path);
    crate::delivery_channel::assert_owned_asset_url(
        channel,
        asset,
        url,
        "WebView2 bootstrapper URL",
    );
    let size = positive_u64(value, "sizeBytes", path);
    let publisher = field_str(value, "expectedPublisher", path);
    assert_eq!(publisher, "Microsoft Corporation");
    format!(
        "const WEBVIEW2_BOOTSTRAPPER_DELIVERY: Option<WebView2BootstrapperDelivery> = Some(WebView2BootstrapperDelivery {{ version: {version:?}, asset: {asset:?}, download_url: {url:?}, size_bytes: {size}, sha256: {digest:?}, expected_publisher: {publisher:?} }});\n"
    )
}

fn field_str<'a>(value: &'a Map<String, Value>, field: &str, path: &Path) -> &'a str {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| panic!("{} is missing {field}", path.display()))
}

fn field_u64(value: &Map<String, Value>, field: &str, path: &Path) -> u64 {
    value
        .get(field)
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("{} is missing {field}", path.display()))
}

fn positive_u64(value: &Map<String, Value>, field: &str, path: &Path) -> u64 {
    let result = field_u64(value, field, path);
    assert!(result > 0, "{} has invalid {field}", path.display());
    result
}

fn identifier(value: &str, path: &Path) {
    assert!(
        value.len() <= 80
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')),
        "{} has an invalid identifier",
        path.display()
    );
}

fn sha256(value: &str, path: &Path) {
    assert!(
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "{} has an invalid SHA-256",
        path.display()
    );
}

fn relative_path(value: &str, path: &Path) {
    assert!(
        !value.is_empty()
            && value.len() <= 512
            && !value.contains('\\')
            && value
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != ".."),
        "{} has an unsafe path",
        path.display()
    );
}
