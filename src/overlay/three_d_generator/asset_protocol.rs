use std::borrow::Cow;
use std::collections::HashMap;
use std::io::{Cursor, Read, Seek};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use serde_json::{Value, json};
use wry::http::{Request, Response};

#[path = "asset_accessor_validation.rs"]
mod accessors;
#[path = "asset_feature_validation.rs"]
mod features;
#[path = "asset_float_validation.rs"]
mod floats;
#[path = "asset_protocol_semantics.rs"]
mod semantics;
#[path = "asset_skin_validation.rs"]
mod skins;

const MAX_GLB_BYTES: u64 = crate::overlay::generation_history::THREE_D_PREVIEW_MAX_BYTES;
const MAX_GLB_JSON_BYTES: u64 = 8 * 1024 * 1024;
const MAX_EMBEDDED_URI_BYTES: usize = 2_800_000;
const MAX_TOTAL_BUFFER_VIEW_BYTES: u64 = MAX_GLB_BYTES;
const MAX_GLTF_BUFFERS: usize = 64;
const MAX_GLTF_BUFFER_VIEWS: usize = 32_768;
const MAX_GLTF_NODES: usize = 4_096;
const MAX_GLTF_SCENES: usize = 64;
const MAX_GLTF_MESHES: usize = 1_024;
const MAX_GLTF_PRIMITIVES: usize = 4_096;
const MAX_GLTF_MATERIALS: usize = 1_024;
const MAX_GLTF_ACCESSORS: usize = 16_384;
const MAX_GLTF_ACCESSOR_ELEMENTS: u64 = 12_000_000;
const MAX_GLTF_VERTICES: u64 = 2_000_000;
const MAX_GLTF_INDICES: u64 = 6_000_000;
const MAX_GLTF_MORPH_TARGETS: usize = 256;
const MAX_GLTF_MORPH_ELEMENTS: u64 = 8_000_000;
const MAX_GLTF_SKINS: usize = 64;
const MAX_GLTF_JOINTS_PER_SKIN: usize = 512;
const MAX_GLTF_TOTAL_JOINTS: usize = MAX_GLTF_NODES;
const MAX_PRIMITIVE_ATTRIBUTES: usize = 16;
const MAX_MORPH_ATTRIBUTES: usize = 8;
pub(super) const MAX_GLTF_ABSOLUTE_RENDERER_VALUE: f64 = 10_000_000.0;
const MAX_GLTF_NODE_DEPTH: usize = 256;
const JSON_CHUNK: u32 = 0x4e4f_534a;
const BIN_CHUNK: u32 = 0x004e_4942;

static TOKENS: LazyLock<Mutex<HashMap<String, PathBuf>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static VALIDATION_LANE: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
static TOKEN_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Default)]
struct AccessorInfo {
    count: u64,
    component_type: u64,
    component_count: u64,
    buffer_view: usize,
    byte_offset: u64,
    absolute_offset: u64,
    byte_stride: Option<u64>,
    normalized: bool,
}

#[derive(Clone, Copy)]
struct BufferViewInfo {
    buffer: usize,
    byte_offset: u64,
    length: u64,
    byte_stride: Option<u64>,
}

fn response(status: u16, body: Cow<'static, [u8]>) -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(status)
        .header("Content-Type", "model/gltf-binary")
        .header("Access-Control-Allow-Origin", "*")
        .header("Cache-Control", "no-store")
        .body(body)
        .unwrap_or_else(|_| Response::new(Cow::Borrowed(b"Internal Error")))
}

pub(super) fn validate_glb(path: &Path) -> Result<PathBuf, String> {
    let _permit = VALIDATION_LANE
        .lock()
        .unwrap_or_else(|value| value.into_inner());
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "The model result is no longer available.".to_string())?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        return Err("The model result is invalid.".to_string());
    }
    let canonical = std::fs::canonicalize(path)
        .map_err(|_| "The model result is no longer available.".to_string())?;
    let mut file = std::fs::File::open(&canonical)
        .map_err(|_| "The model result is no longer available.".to_string())?;
    validate_glb_handle(&mut file, &canonical)?;
    Ok(canonical)
}

#[cfg(windows)]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn validate_glb_handle(file: &mut std::fs::File, canonical: &Path) -> Result<u64, String> {
    let metadata = file
        .metadata()
        .map_err(|_| "The model result is no longer available.".to_string())?;
    if !metadata.is_file() {
        return Err("The model result is invalid.".to_string());
    }
    validate_glb_reader(file, canonical, metadata.len())
}

fn validate_glb_reader(
    file: &mut (impl Read + Seek),
    canonical: &Path,
    file_length: u64,
) -> Result<u64, String> {
    if !(20..=MAX_GLB_BYTES).contains(&file_length)
        || !canonical
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("glb"))
    {
        return Err("The model result is invalid.".to_string());
    }
    let mut header = [0_u8; 12];
    file.rewind()
        .and_then(|_| file.read_exact(&mut header))
        .map_err(|_| "The model result header is invalid.".to_string())?;
    let version = u32::from_le_bytes(header[4..8].try_into().unwrap_or_default());
    let declared_length = u32::from_le_bytes(header[8..12].try_into().unwrap_or_default());
    if &header[..4] != b"glTF" || version != 2 || u64::from(declared_length) != file_length {
        return Err("The model result header is invalid.".to_string());
    }
    let mut position = 12_u64;
    let mut chunk_index = 0_usize;
    let mut description = None;
    let mut binary = None;
    while position < file_length {
        let mut chunk_header = [0_u8; 8];
        file.read_exact(&mut chunk_header)
            .map_err(|_| "The model result chunk table is invalid.".to_string())?;
        let chunk_length = u64::from(u32::from_le_bytes(
            chunk_header[..4].try_into().unwrap_or_default(),
        ));
        let chunk_type = u32::from_le_bytes(chunk_header[4..].try_into().unwrap_or_default());
        position = position.saturating_add(8);
        if chunk_length % 4 != 0
            || position.saturating_add(chunk_length) > file_length
            || chunk_index > 1
            || chunk_index == 0 && chunk_type != JSON_CHUNK
            || chunk_index == 1 && chunk_type != BIN_CHUNK
        {
            return Err("The model result chunk table is invalid.".to_string());
        }
        if chunk_type == JSON_CHUNK {
            if chunk_length == 0 || chunk_length > MAX_GLB_JSON_BYTES {
                return Err("The model result description is invalid.".to_string());
            }
            let mut json_bytes = vec![0_u8; chunk_length as usize];
            file.read_exact(&mut json_bytes)
                .map_err(|_| "The model result description is invalid.".to_string())?;
            let json_end = json_object_end(&json_bytes)
                .ok_or_else(|| "The model result description is invalid.".to_string())?;
            if json_bytes[json_end..].iter().any(|byte| *byte != b' ') {
                return Err("The model result description is invalid.".to_string());
            }
            let json: Value = serde_json::from_slice(&json_bytes[..json_end])
                .map_err(|_| "The model result description is invalid.".to_string())?;
            validate_embedded_uris(&json)?;
            description = Some(json);
        } else {
            let mut bytes = vec![0_u8; chunk_length as usize];
            file.read_exact(&mut bytes)
                .map_err(|_| "The model result chunk table is invalid.".to_string())?;
            binary = Some(bytes);
        }
        position = position.saturating_add(chunk_length);
        chunk_index += 1;
    }
    if position != file_length || chunk_index == 0 {
        return Err("The model result chunk table is invalid.".to_string());
    }
    validate_gltf_semantics(
        description
            .as_ref()
            .ok_or_else(|| "The model result description is invalid.".to_string())?,
        binary.as_deref(),
    )?;
    Ok(file_length)
}

fn json_object_end(bytes: &[u8]) -> Option<usize> {
    let mut started = false;
    let mut depth = 0_usize;
    let mut in_string = false;
    let mut escaped = false;
    for (index, byte) in bytes.iter().copied().enumerate() {
        if !started {
            if byte.is_ascii_whitespace() {
                continue;
            }
            if byte != b'{' {
                return None;
            }
            started = true;
            depth = 1;
            continue;
        }
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' | b'[' => depth = depth.checked_add(1)?,
            b'}' | b']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index + 1);
                }
            }
            _ => {}
        }
    }
    None
}

fn validate_gltf_semantics(value: &Value, binary: Option<&[u8]>) -> Result<(), String> {
    let invalid = || "The model result description is invalid.".to_string();
    let root = value.as_object().ok_or_else(invalid)?;
    let version = root
        .get("asset")
        .and_then(Value::as_object)
        .and_then(|asset| asset.get("version"))
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;
    if version != "2.0"
        || root
            .get("asset")
            .and_then(Value::as_object)
            .and_then(|asset| asset.get("minVersion"))
            .is_some_and(|value| value.as_str() != Some("2.0"))
    {
        return Err(invalid());
    }
    features::validate(root)?;
    let buffers = root
        .get("buffers")
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty() && items.len() <= MAX_GLTF_BUFFERS)
        .ok_or_else(invalid)?;
    let buffer_zero_uses_binary = buffers[0]
        .as_object()
        .is_some_and(|buffer| !buffer.contains_key("uri"));
    if binary.is_some() != buffer_zero_uses_binary {
        return Err(invalid());
    }
    let mut buffer_lengths = Vec::with_capacity(buffers.len());
    for (index, buffer) in buffers.iter().enumerate() {
        let buffer = buffer.as_object().ok_or_else(invalid)?;
        let length = buffer
            .get("byteLength")
            .and_then(Value::as_u64)
            .filter(|length| *length > 0 && *length <= MAX_GLB_BYTES)
            .ok_or_else(invalid)?;
        if buffer.get("uri").is_none() {
            let bytes = binary.filter(|_| index == 0).ok_or_else(invalid)?;
            let padded_length = length
                .checked_add(3)
                .map(|value| value & !3)
                .ok_or_else(invalid)?;
            let logical_length = usize::try_from(length).map_err(|_| invalid())?;
            if bytes.len() as u64 != padded_length
                || bytes
                    .get(logical_length..)
                    .is_none_or(|padding| padding.iter().any(|byte| *byte != 0))
            {
                return Err(invalid());
            }
        }
        buffer_lengths.push(length);
    }
    let views = root
        .get("bufferViews")
        .and_then(Value::as_array)
        .ok_or_else(invalid)?;
    if views.len() > MAX_GLTF_BUFFER_VIEWS {
        return Err(invalid());
    }
    let mut view_lengths = Vec::with_capacity(views.len());
    let mut total_view_bytes = 0_u64;
    for view in views {
        let view = view.as_object().ok_or_else(invalid)?;
        let buffer = view
            .get("buffer")
            .and_then(Value::as_u64)
            .and_then(|index| usize::try_from(index).ok())
            .filter(|index| *index < buffer_lengths.len())
            .ok_or_else(invalid)?;
        let offset = view.get("byteOffset").and_then(Value::as_u64).unwrap_or(0);
        let length = view
            .get("byteLength")
            .and_then(Value::as_u64)
            .filter(|length| *length > 0)
            .ok_or_else(invalid)?;
        total_view_bytes = total_view_bytes
            .checked_add(length)
            .filter(|total| *total <= MAX_TOTAL_BUFFER_VIEW_BYTES)
            .ok_or_else(invalid)?;
        if offset
            .checked_add(length)
            .is_none_or(|end| end > buffer_lengths[buffer])
        {
            return Err(invalid());
        }
        let byte_stride = match view.get("byteStride") {
            Some(value) => Some(
                value
                    .as_u64()
                    .filter(|stride| (4..=252).contains(stride) && stride % 4 == 0)
                    .ok_or_else(invalid)?,
            ),
            None => None,
        };
        view_lengths.push(BufferViewInfo {
            buffer,
            byte_offset: offset,
            length,
            byte_stride,
        });
    }
    let accessor_info = accessors::validate(root, &view_lengths)?;
    semantics::validate(root, &accessor_info)?;
    let buffers = super::asset_texture_validation::resolve_buffers(root, binary)?;
    semantics::validate_indices(root, &accessor_info, &view_lengths, &buffers)?;
    skins::validate(root, &accessor_info, &view_lengths, &buffers)?;
    floats::validate(root, &accessor_info, &view_lengths, &buffers)?;
    super::asset_texture_validation::validate_textures(root, binary)?;
    Ok(())
}

fn validate_embedded_uris(value: &Value) -> Result<(), String> {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if key == "uri" {
                    let uri = value
                        .as_str()
                        .ok_or_else(|| "The model contains an invalid resource.".to_string())?;
                    let lower = uri
                        .get(..uri.len().min(64))
                        .unwrap_or(uri)
                        .to_ascii_lowercase();
                    let embedded = lower.starts_with("data:application/octet-stream;base64,")
                        || lower.starts_with("data:image/png;base64,")
                        || lower.starts_with("data:image/jpeg;base64,")
                        || lower.starts_with("data:image/webp;base64,");
                    if uri.len() > MAX_EMBEDDED_URI_BYTES || !embedded {
                        return Err("The model cannot load external resources.".to_string());
                    }
                } else {
                    validate_embedded_uris(value)?;
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                validate_embedded_uris(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn validate_generated(path: &str, output_dir: &Path) -> Result<PathBuf, String> {
    let result = validate_glb(Path::new(path))?;
    let directory = std::fs::canonicalize(output_dir)
        .map_err(|_| "The model result folder is unavailable.".to_string())?;
    if result.parent() != Some(directory.as_path()) {
        return Err("The model result is outside the selected folder.".to_string());
    }
    Ok(result)
}

pub(super) fn validate_generated_exact(
    path: &str,
    output_dir: &Path,
    output_name: &str,
) -> Result<PathBuf, String> {
    let expected = crate::overlay::creation_output::assigned_path(output_dir, output_name)?;
    let result = validate_generated(path, output_dir)?;
    let expected = std::fs::canonicalize(expected)
        .map_err(|_| "The assigned model result is unavailable.".to_string())?;
    if result != expected {
        return Err("The model result does not match its assigned file.".to_string());
    }
    Ok(result)
}

pub(super) fn issue(path: &str) -> Result<Value, String> {
    let path = validate_glb(Path::new(path))?;
    if !super::runtime::is_known_result_path(&path) {
        return Err("The model result is not available in this session.".to_string());
    }
    let token = format!(
        "{:x}{:x}{:x}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        TOKEN_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let mut tokens = TOKENS.lock().unwrap_or_else(|value| value.into_inner());
    tokens.clear();
    tokens.insert(token.clone(), path);
    Ok(json!({ "url": model_asset_url(&token) }))
}

fn model_asset_url(token: &str) -> String {
    let origin = if cfg!(windows) {
        "http://sgt3d.localhost"
    } else {
        "sgt3d://localhost"
    };
    format!("{origin}/model/{token}")
}

pub(super) fn clear() {
    TOKENS
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .clear();
}

pub(super) fn handle(request: Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
    if request.headers().contains_key("range") {
        return response(416, Cow::Borrowed(b"Range requests are not supported."));
    }
    let Some(token) = request.uri().path().strip_prefix("/model/") else {
        return response(404, Cow::Borrowed(b"Not Found"));
    };
    if token.is_empty() || token.contains('/') {
        return response(404, Cow::Borrowed(b"Not Found"));
    }
    let path = TOKENS
        .lock()
        .unwrap_or_else(|value| value.into_inner())
        .remove(token);
    let Some(path) = path else {
        return response(404, Cow::Borrowed(b"Not Found"));
    };
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_file() && !is_reparse_point(&metadata) => metadata,
        _ => return response(404, Cow::Borrowed(b"Not Found")),
    };
    let canonical = match std::fs::canonicalize(&path) {
        Ok(canonical) if canonical == path => canonical,
        Err(_) => return response(404, Cow::Borrowed(b"Not Found")),
        _ => return response(404, Cow::Borrowed(b"Not Found")),
    };
    let mut file = match std::fs::File::open(&canonical) {
        Ok(file) => file,
        Err(_) => return response(404, Cow::Borrowed(b"Not Found")),
    };
    let bytes = {
        let _permit = VALIDATION_LANE
            .lock()
            .unwrap_or_else(|value| value.into_inner());
        let length = match file.metadata() {
            Ok(current)
                if current.is_file()
                    && current.len() == metadata.len()
                    && current.len() >= 20
                    && current.len() <= MAX_GLB_BYTES =>
            {
                current.len()
            }
            _ => return response(404, Cow::Borrowed(b"Not Found")),
        };
        if file.rewind().is_err() {
            return response(404, Cow::Borrowed(b"Not Found"));
        }
        let mut bytes = Vec::with_capacity(length as usize);
        if file
            .take(MAX_GLB_BYTES + 1)
            .read_to_end(&mut bytes)
            .is_err()
            || bytes.len() as u64 != length
        {
            return response(404, Cow::Borrowed(b"Not Found"));
        }
        let mut snapshot = Cursor::new(bytes.as_slice());
        if validate_glb_reader(&mut snapshot, &canonical, length).is_err() {
            return response(404, Cow::Borrowed(b"Not Found"));
        }
        bytes
    };
    debug_assert_eq!(bytes.len() as u64, metadata.len());
    response(200, Cow::Owned(bytes))
}

#[cfg(test)]
#[path = "asset_protocol_hostile_tests.rs"]
mod hostile_tests;
#[cfg(test)]
#[path = "asset_protocol_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "asset_protocol_path_tests.rs"]
mod path_tests;
#[cfg(test)]
#[path = "asset_transport_tests.rs"]
mod transport_tests;
