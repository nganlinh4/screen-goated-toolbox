use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use base64::{Engine as _, engine::general_purpose};
use quick_xml::{
    Reader,
    encoding::Decoder,
    events::{BytesStart, Event},
};
use serde_json::{Value, json};

const MAX_SVG_BYTES: u64 = crate::overlay::generation_history::SVG_RESULT_RESERVATION_BYTES;
const MAX_SVG_ELEMENTS: usize = 50_000;
const MAX_SVG_ATTRIBUTES: usize = 250_000;
const MAX_EMBEDDED_RASTER_PIXELS: u64 = 16_000_000;
const MAX_TOTAL_EMBEDDED_RASTER_PIXELS: u64 = 32_000_000;
const MAX_EDIT_SOURCE_BYTES: usize = 2 * 1024 * 1024;
const MAX_EDITABLE_GEOMETRY: usize = 5_000;

static EDIT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const BLOCKED_ELEMENTS: &[&str] = &[
    "animate",
    "animatemotion",
    "animatetransform",
    "audio",
    "canvas",
    "discard",
    "embed",
    "feimage",
    "foreignobject",
    "iframe",
    "include",
    "handler",
    "listener",
    "object",
    "script",
    "set",
    "video",
];

pub(super) fn bounded_embedded_raster_pixels(value: &str) -> Option<u64> {
    if value.len() > 2_800_000
        || !value
            .get(.."data:image/".len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:image/"))
    {
        return None;
    }
    let lower = value[..value.len().min(64)].to_ascii_lowercase();
    let expected_format = if lower.starts_with("data:image/png;base64,") {
        image::ImageFormat::Png
    } else if lower.starts_with("data:image/jpeg;base64,") {
        image::ImageFormat::Jpeg
    } else {
        return None;
    };
    let (_, payload) = value.split_once(',')?;
    let Ok(bytes) = general_purpose::STANDARD.decode(payload) else {
        return None;
    };
    if expected_format == image::ImageFormat::Png && png_is_animated(&bytes) {
        return None;
    }
    let Ok(reader) = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()
    else {
        return None;
    };
    if reader.format() != Some(expected_format) {
        return None;
    }
    let (width, height) = reader.into_dimensions().ok()?;
    let pixels = u64::from(width).checked_mul(u64::from(height))?;
    (width > 0 && height > 0 && pixels <= MAX_EMBEDDED_RASTER_PIXELS).then_some(pixels)
}

fn png_is_animated(bytes: &[u8]) -> bool {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return false;
    }
    let mut offset = 8_usize;
    while offset.checked_add(12).is_some_and(|end| end <= bytes.len()) {
        let length =
            u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap_or_default()) as usize;
        let kind = &bytes[offset + 4..offset + 8];
        if kind == b"acTL" {
            return true;
        }
        let Some(end) = offset
            .checked_add(12)
            .and_then(|value| value.checked_add(length))
        else {
            return true;
        };
        if end > bytes.len() || kind == b"IEND" {
            break;
        }
        offset = end;
    }
    false
}

fn add_embedded_raster_pixels(total: &mut u64, pixels: u64) -> Result<(), String> {
    *total = total
        .checked_add(pixels)
        .ok_or_else(|| "SVG embedded images are too large to preview safely.".to_string())?;
    if *total > MAX_TOTAL_EMBEDDED_RASTER_PIXELS {
        return Err("SVG embedded images are too large to preview safely.".to_string());
    }
    Ok(())
}

fn validate_element(
    element: &BytesStart<'_>,
    decoder: Decoder,
    inherited_xlink_namespace: bool,
    root_seen: &mut bool,
    element_count: &mut usize,
    attribute_count: &mut usize,
    embedded_raster_pixels: &mut u64,
) -> Result<(bool, bool), String> {
    *element_count += 1;
    if *element_count > MAX_SVG_ELEMENTS {
        return Err("SVG contains too many elements to preview safely.".to_string());
    }
    let name = String::from_utf8_lossy(element.local_name().as_ref()).to_ascii_lowercase();
    let is_root = !*root_seen;
    let mut canonical_root_namespace = false;
    let mut declares_xlink_namespace = false;
    if element.name().as_ref().contains(&b':') {
        return Err("SVG contains an unsupported namespace.".to_string());
    }
    if !*root_seen {
        if name != "svg" {
            return Err("SVG root element is missing.".to_string());
        }
        *root_seen = true;
    }
    if BLOCKED_ELEMENTS.contains(&name.as_str()) || name == "filter" || name.starts_with("fe") {
        return Err("SVG contains unsupported active content.".to_string());
    }
    let mut attributes = Vec::new();
    for attribute in element.attributes().with_checks(true) {
        *attribute_count += 1;
        if *attribute_count > MAX_SVG_ATTRIBUTES {
            return Err("SVG contains too many attributes to preview safely.".to_string());
        }
        let attribute = attribute.map_err(|_| "SVG contains an invalid attribute.".to_string())?;
        let key = std::str::from_utf8(attribute.key.as_ref())
            .map_err(|_| "SVG contains an invalid attribute.".to_string())?
            .to_string();
        let value = attribute
            .decode_and_unescape_value(decoder)
            .map_err(|_| "SVG contains an invalid attribute value.".to_string())?
            .into_owned();
        attributes.push((key, value));
    }
    for (key, value) in &attributes {
        if key.eq_ignore_ascii_case("xmlns") && key != "xmlns"
            || key
                .get(.."xmlns:".len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("xmlns:"))
                && key != "xmlns:xlink"
        {
            return Err("SVG contains an unsupported namespace.".to_string());
        }
        if key == "xmlns" {
            if value.trim() != "http://www.w3.org/2000/svg" {
                return Err("SVG contains an unsupported namespace.".to_string());
            }
            canonical_root_namespace |= is_root;
        } else if key == "xmlns:xlink" {
            if value.trim() != "http://www.w3.org/1999/xlink" {
                return Err("SVG contains an unsupported namespace.".to_string());
            }
            declares_xlink_namespace = true;
        }
    }
    let xlink_namespace = inherited_xlink_namespace || declares_xlink_namespace;
    for (raw_key, value) in attributes {
        if raw_key == "xmlns" || raw_key == "xmlns:xlink" {
            continue;
        }
        if raw_key.contains(':')
            && !matches!(raw_key.as_str(), "xml:lang" | "xml:space")
            && !(raw_key == "xlink:href" && xlink_namespace)
        {
            return Err("SVG contains an unsupported namespace.".to_string());
        }
        let key = raw_key.to_ascii_lowercase();
        let normalized = value.trim().to_ascii_lowercase();
        let embedded_pixels = (name == "image" && matches!(key.as_str(), "href" | "xlink:href"))
            .then(|| bounded_embedded_raster_pixels(&value))
            .flatten();
        if key.starts_with("on")
            || key == "filter"
            || matches!(
                key.as_str(),
                "src" | "data" | "poster" | "formaction" | "xml:base"
            )
            || (matches!(key.as_str(), "href" | "xlink:href")
                && !normalized.starts_with('#')
                && embedded_pixels.is_none())
            || normalized.contains("javascript:")
            || normalized.contains("vbscript:")
            || normalized.contains("data:text/html")
            || normalized.contains("@import")
            || normalized.contains("expression(")
            || normalized.contains("-moz-binding")
            || normalized.contains("behavior:")
            || normalized.contains("/*")
            || normalized.contains('\\')
        {
            return Err("SVG contains unsupported active content.".to_string());
        }
        if let Some(pixels) = embedded_pixels {
            add_embedded_raster_pixels(embedded_raster_pixels, pixels)?;
        }
        if key == "style" {
            super::svg_security::validate_css(&value, false)?;
        } else {
            super::svg_security::validate_url_functions(&normalized)?;
        }
    }
    if is_root && !canonical_root_namespace {
        return Err("SVG root namespace is missing or unsupported.".to_string());
    }
    Ok((name == "style", xlink_namespace))
}

pub(in crate::overlay::image_to_svg) fn validate_svg(svg: &str) -> Result<(), String> {
    if svg.is_empty() || svg.len() as u64 > MAX_SVG_BYTES {
        return Err("SVG is empty or too large.".to_string());
    }
    let document_text = svg.to_ascii_lowercase();
    if document_text.contains("@import")
        || document_text.contains("javascript:")
        || document_text.contains("vbscript:")
        || document_text.contains("expression(")
    {
        return Err("SVG contains unsupported active content.".to_string());
    }
    let mut reader = Reader::from_str(svg);
    reader.config_mut().check_end_names = true;
    let mut root_seen = false;
    let mut element_count = 0usize;
    let mut attribute_count = 0usize;
    let mut embedded_raster_pixels = 0_u64;
    let mut style_depth = 0usize;
    let mut depth = 0usize;
    let mut root_closed = false;
    let mut xlink_namespace_stack = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => {
                if root_closed {
                    return Err("SVG contains content outside its root element.".to_string());
                }
                let (is_style, xlink_namespace) = validate_element(
                    &element,
                    reader.decoder(),
                    xlink_namespace_stack.last().copied().unwrap_or(false),
                    &mut root_seen,
                    &mut element_count,
                    &mut attribute_count,
                    &mut embedded_raster_pixels,
                )?;
                if is_style {
                    style_depth += 1;
                }
                xlink_namespace_stack.push(xlink_namespace);
                depth += 1;
            }
            Ok(Event::Empty(element)) => {
                if root_closed {
                    return Err("SVG contains content outside its root element.".to_string());
                }
                validate_element(
                    &element,
                    reader.decoder(),
                    xlink_namespace_stack.last().copied().unwrap_or(false),
                    &mut root_seen,
                    &mut element_count,
                    &mut attribute_count,
                    &mut embedded_raster_pixels,
                )?;
                if depth == 0 {
                    root_closed = true;
                }
            }
            Ok(Event::End(element)) => {
                if depth == 0 {
                    return Err("SVG document has an unmatched closing element.".to_string());
                }
                if element.local_name().as_ref().eq_ignore_ascii_case(b"style") {
                    style_depth = style_depth.saturating_sub(1);
                }
                xlink_namespace_stack
                    .pop()
                    .ok_or_else(|| "SVG document has an unmatched closing element.".to_string())?;
                depth -= 1;
                if depth == 0 {
                    root_closed = true;
                }
            }
            Ok(Event::Text(text)) if style_depth > 0 => {
                let decoded = text
                    .decode()
                    .map_err(|_| "SVG contains invalid style text.".to_string())?;
                let unescaped = quick_xml::escape::unescape(&decoded)
                    .map_err(|_| "SVG contains invalid style text.".to_string())?;
                super::svg_security::validate_css(&unescaped, true)?;
            }
            Ok(Event::CData(text)) if style_depth > 0 => {
                let decoded = text
                    .decode()
                    .map_err(|_| "SVG contains invalid style text.".to_string())?;
                super::svg_security::validate_css(&decoded, true)?;
            }
            Ok(Event::Text(text)) if depth == 0 => {
                if !text
                    .decode()
                    .map_err(|_| "SVG contains invalid text.".to_string())?
                    .trim()
                    .is_empty()
                {
                    return Err("SVG contains content outside its root element.".to_string());
                }
            }
            Ok(Event::CData(text)) if depth == 0 => {
                if !text
                    .decode()
                    .map_err(|_| "SVG contains invalid text.".to_string())?
                    .trim()
                    .is_empty()
                {
                    return Err("SVG contains content outside its root element.".to_string());
                }
            }
            Ok(Event::DocType(_)) | Ok(Event::PI(_)) => {
                return Err("SVG contains unsupported document directives.".to_string());
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => return Err("SVG document is malformed.".to_string()),
        }
    }
    (root_seen && root_closed && depth == 0 && xlink_namespace_stack.is_empty())
        .then_some(())
        .ok_or_else(|| "SVG root element is missing.".to_string())?;
    super::svg_security::validate(svg)?;
    super::svg_expansion::validate(svg)
}

fn validate_svg_edit(svg: &str) -> Result<(), String> {
    if svg.len() > MAX_EDIT_SOURCE_BYTES {
        return Err("This complex SVG is available as a preview only.".to_string());
    }
    validate_svg(svg)?;
    let mut reader = Reader::from_str(svg);
    let mut geometry = 0usize;
    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) | Ok(Event::Empty(element)) => {
                let name =
                    String::from_utf8_lossy(element.local_name().as_ref()).to_ascii_lowercase();
                if matches!(
                    name.as_str(),
                    "path" | "rect" | "circle" | "ellipse" | "polygon" | "polyline" | "line"
                ) {
                    geometry += 1;
                    if geometry > MAX_EDITABLE_GEOMETRY {
                        return Err("This complex SVG is available as a preview only.".to_string());
                    }
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => return Err("SVG document is malformed.".to_string()),
        }
    }
    Ok(())
}

fn read_valid_svg(path: &Path) -> Result<String, String> {
    let file = std::fs::File::open(path)
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("Could not inspect {}: {error}", path.display()))?;
    if !metadata.is_file() || metadata.len() > MAX_SVG_BYTES {
        return Err("Vector preview is unavailable or too large.".to_string());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_SVG_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read vector: {error}"))?;
    if bytes.len() as u64 > MAX_SVG_BYTES || bytes.len() as u64 != metadata.len() {
        return Err("Vector preview changed or is too large.".to_string());
    }
    let text =
        String::from_utf8(bytes).map_err(|_| "Vector preview is not valid UTF-8.".to_string())?;
    validate_svg(&text)?;
    Ok(text)
}

pub(in crate::overlay::image_to_svg) fn validate_generated_result(
    mut value: Value,
    output_dir: &Path,
    output_name: &str,
) -> Result<Value, String> {
    let requested = value
        .get("outputPath")
        .and_then(Value::as_str)
        .ok_or_else(|| "Creation engine did not return a vector file.".to_string())?;
    let requested_path = Path::new(requested);
    let metadata = std::fs::symlink_metadata(requested_path)
        .map_err(|_| "Creation engine did not return a vector file.".to_string())?;
    if !metadata.is_file() || super::svg_security::is_reparse_point(&metadata) {
        return Err("Creation engine returned an invalid vector file.".to_string());
    }
    let output_path = std::fs::canonicalize(requested)
        .map_err(|error| format!("Could not open generated vector: {error}"))?;
    let canonical_dir = std::fs::canonicalize(output_dir)
        .map_err(|error| format!("Could not verify result folder: {error}"))?;
    if output_path.parent() != Some(canonical_dir.as_path())
        || !output_path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("svg"))
    {
        return Err("Creation engine returned a result outside the selected folder.".to_string());
    }
    let expected = crate::overlay::creation_output::assigned_path(output_dir, output_name)?;
    let expected = std::fs::canonicalize(expected)
        .map_err(|_| "Creation engine did not return the assigned vector file.".to_string())?;
    if output_path != expected {
        return Err("Creation engine returned a different vector file.".to_string());
    }
    read_valid_svg(&output_path)?;
    let name = output_path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .ok_or_else(|| "Generated vector filename is missing.".to_string())?;
    value["outputPath"] = Value::String(output_path.to_string_lossy().to_string());
    value["outputName"] = Value::String(name);
    Ok(value)
}

pub(in crate::overlay::image_to_svg) fn read_asset(path: &str) -> Result<Value, String> {
    let path = std::fs::canonicalize(path)
        .map_err(|_| "Vector result is no longer available.".to_string())?;
    if !super::is_known_result_path(&path) {
        return Err("Vector result is not available in this session.".to_string());
    }
    let metadata = std::fs::metadata(&path)
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    if path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("svg"))
    {
        let text = read_valid_svg(&path)?;
        return Ok(json!({ "text": text, "sizeBytes": metadata.len() }));
    }
    Err("This result is not a supported SVG image.".to_string())
}

pub(in crate::overlay::image_to_svg) fn save_svg_edits(
    path: &str,
    svg: &str,
) -> Result<Value, String> {
    let path = std::fs::canonicalize(path)
        .map_err(|_| "Vector result is no longer available.".to_string())?;
    if !super::is_known_result_path(&path) {
        return Err("Vector result is not available in this session.".to_string());
    }
    write_svg_edits(&path, svg)
}

fn write_svg_edits(path: &Path, svg: &str) -> Result<Value, String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("Could not open {}: {error}", path.display()))?;
    let is_svg = path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("svg"));
    if !metadata.is_file() || !is_svg {
        return Err("Only an existing SVG result can be edited.".to_string());
    }
    validate_svg_edit(svg)?;
    atomic_replace_existing(path, svg.as_bytes())?;
    Ok(json!({ "sizeBytes": svg.len() }))
}

fn atomic_replace_existing(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Vector result folder is unavailable.".to_string())?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Vector result filename is invalid.".to_string())?;
    let mut temporary = None;
    for _ in 0..32 {
        let candidate = parent.join(format!(
            ".{name}.edit-{}-{}.tmp",
            std::process::id(),
            EDIT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(mut file) => {
                if let Err(error) = file
                    .write_all(bytes)
                    .and_then(|_| file.flush())
                    .and_then(|_| file.sync_all())
                {
                    drop(file);
                    let _ = std::fs::remove_file(&candidate);
                    return Err(format!("Could not save {}: {error}", path.display()));
                }
                drop(file);
                temporary = Some(candidate);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(format!("Could not save {}: {error}", path.display())),
        }
    }
    let temporary = temporary
        .ok_or_else(|| "Could not allocate a temporary vector result file.".to_string())?;
    let result = replace_existing(path, &temporary);
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[cfg(windows)]
fn replace_existing(path: &Path, replacement: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt as _;
    use windows::Win32::Storage::FileSystem::{REPLACEFILE_WRITE_THROUGH, ReplaceFileW};
    use windows::core::PCWSTR;

    let target = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let replacement = replacement
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    unsafe {
        ReplaceFileW(
            PCWSTR(target.as_ptr()),
            PCWSTR(replacement.as_ptr()),
            PCWSTR::null(),
            REPLACEFILE_WRITE_THROUGH,
            None,
            None,
        )
    }
    .map_err(|error| format!("Could not replace {}: {error}", path.display()))
}

#[cfg(not(windows))]
fn replace_existing(path: &Path, replacement: &Path) -> Result<(), String> {
    std::fs::rename(replacement, path)
        .map_err(|error| format!("Could not replace {}: {error}", path.display()))
}

pub(in crate::overlay::image_to_svg) fn open_output(
    requested_path: Option<&str>,
) -> Result<(), String> {
    let path = requested_path
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(super::default_output_dir);
    let target = if path.is_file() {
        path.parent()
            .map(PathBuf::from)
            .unwrap_or_else(super::default_output_dir)
    } else {
        path
    };
    open::that(&target).map_err(|error| format!("Could not open {}: {error}", target.display()))
}

#[cfg(test)]
#[path = "asset_io_tests.rs"]
mod tests;
