use std::borrow::Cow;
use std::io::Cursor;

use base64::{Engine as _, engine::general_purpose};
use image::{ImageFormat, ImageReader};
use serde_json::{Map, Value};

#[path = "asset_material_validation.rs"]
mod materials;
use materials::validate_material_texture_references;

pub(super) const MAX_TEXTURE_AXIS: u32 = 8_192;
pub(super) const MAX_TEXTURE_PIXELS: u64 = 16 * 1024 * 1024;
pub(super) const MAX_TOTAL_TEXTURE_PIXELS: u64 = 32 * 1024 * 1024;
pub(super) const MAX_TEXTURE_IMAGES: usize = 256;
pub(super) const MAX_TEXTURES: usize = 512;
pub(super) const MAX_TEXTURE_SAMPLERS: usize = 128;

pub(super) fn validate_textures(
    root: &Map<String, Value>,
    binary: Option<&[u8]>,
) -> Result<(), String> {
    let images = match root.get("images") {
        Some(value) => value
            .as_array()
            .filter(|items| items.len() <= MAX_TEXTURE_IMAGES)
            .ok_or_else(invalid)?
            .as_slice(),
        None => &[],
    };
    let buffers = resolve_buffers(root, binary)?;
    let views = root
        .get("bufferViews")
        .and_then(Value::as_array)
        .ok_or_else(invalid)?;
    let mut image_pixels = Vec::with_capacity(images.len());
    let mut total_image_pixels = 0_u64;
    for image in images {
        let image = image.as_object().ok_or_else(invalid)?;
        let (mime, bytes) = match (
            image.get("uri").and_then(Value::as_str),
            image.get("bufferView").and_then(Value::as_u64),
        ) {
            (Some(uri), None) => decode_image_uri(uri)?,
            (None, Some(view)) => {
                let mime = image
                    .get("mimeType")
                    .and_then(Value::as_str)
                    .ok_or_else(invalid)?;
                (mime, Cow::Borrowed(resolve_view(views, &buffers, view)?))
            }
            _ => return Err(invalid()),
        };
        let pixels = validate_texture_payload(mime, &bytes)?;
        total_image_pixels = total_image_pixels
            .checked_add(pixels)
            .filter(|pixels| *pixels <= MAX_TOTAL_TEXTURE_PIXELS)
            .ok_or_else(invalid)?;
        image_pixels.push(pixels);
    }
    let (texture_pixels, referenced_pixels) = validate_texture_table(root, &image_pixels)?;
    validate_material_texture_references(root, &texture_pixels, referenced_pixels)
}

fn validate_texture_table(
    root: &Map<String, Value>,
    image_pixels: &[u64],
) -> Result<(Vec<u64>, u64), String> {
    let sampler_count = match root.get("samplers") {
        Some(value) => {
            let samplers = value
                .as_array()
                .filter(|items| items.len() <= MAX_TEXTURE_SAMPLERS)
                .ok_or_else(invalid)?;
            for sampler in samplers {
                validate_sampler(sampler.as_object().ok_or_else(invalid)?)?;
            }
            samplers.len()
        }
        None => 0,
    };
    let Some(textures) = root.get("textures") else {
        return Ok((Vec::new(), 0));
    };
    let mut total_texture_pixels = 0_u64;
    let mut texture_pixels_by_index = Vec::new();
    for texture in textures
        .as_array()
        .filter(|items| items.len() <= MAX_TEXTURES)
        .ok_or_else(invalid)?
    {
        let texture = texture.as_object().ok_or_else(invalid)?;
        let base_source = texture
            .get("source")
            .map(|value| {
                value
                    .as_u64()
                    .and_then(|index| usize::try_from(index).ok())
                    .ok_or_else(invalid)
            })
            .transpose()?;
        let webp_extension = texture
            .get("extensions")
            .and_then(Value::as_object)
            .and_then(|extensions| extensions.get("EXT_texture_webp"));
        let webp_source = webp_extension
            .map(|value| {
                value
                    .as_object()
                    .and_then(|extension| extension.get("source"))
                    .and_then(Value::as_u64)
                    .and_then(|index| usize::try_from(index).ok())
                    .ok_or_else(invalid)
            })
            .transpose()?;
        let sources = [base_source, webp_source];
        if sources.iter().all(Option::is_none) {
            return Err(invalid());
        }
        let mut texture_pixels = 0_u64;
        for source in sources.into_iter().flatten() {
            texture_pixels =
                texture_pixels.max(image_pixels.get(source).copied().ok_or_else(invalid)?);
        }
        total_texture_pixels = total_texture_pixels
            .checked_add(texture_pixels)
            .filter(|pixels| *pixels <= MAX_TOTAL_TEXTURE_PIXELS)
            .ok_or_else(invalid)?;
        texture_pixels_by_index.push(texture_pixels);
        if let Some(value) = texture.get("sampler")
            && value
                .as_u64()
                .and_then(|index| usize::try_from(index).ok())
                .is_none_or(|index| index >= sampler_count)
        {
            return Err(invalid());
        }
    }
    Ok((texture_pixels_by_index, total_texture_pixels))
}

fn validate_sampler(sampler: &Map<String, Value>) -> Result<(), String> {
    if sampler.get("name").is_some_and(|value| !value.is_string()) {
        return Err(invalid());
    }
    for (field, allowed) in [
        ("magFilter", &[9728_u64, 9729][..]),
        ("minFilter", &[9728, 9729, 9984, 9985, 9986, 9987][..]),
        ("wrapS", &[33071, 33648, 10497][..]),
        ("wrapT", &[33071, 33648, 10497][..]),
    ] {
        if sampler
            .get(field)
            .is_some_and(|value| value.as_u64().is_none_or(|value| !allowed.contains(&value)))
        {
            return Err(invalid());
        }
    }
    Ok(())
}

pub(super) fn resolve_buffers<'a>(
    root: &Map<String, Value>,
    binary: Option<&'a [u8]>,
) -> Result<Vec<Cow<'a, [u8]>>, String> {
    root.get("buffers")
        .and_then(Value::as_array)
        .ok_or_else(invalid)?
        .iter()
        .enumerate()
        .map(|(index, buffer)| {
            let buffer = buffer.as_object().ok_or_else(invalid)?;
            let length = buffer
                .get("byteLength")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(invalid)?;
            let source = if let Some(uri) = buffer.get("uri").and_then(Value::as_str) {
                Cow::Owned(decode_buffer_uri(uri)?)
            } else if index == 0 {
                Cow::Borrowed(
                    binary
                        .and_then(|bytes| bytes.get(..length))
                        .ok_or_else(invalid)?,
                )
            } else {
                return Err(invalid());
            };
            if source.len() != length {
                return Err(invalid());
            }
            Ok(source)
        })
        .collect()
}

fn resolve_view<'a>(
    views: &[Value],
    buffers: &'a [Cow<'_, [u8]>],
    index: u64,
) -> Result<&'a [u8], String> {
    let view = usize::try_from(index)
        .ok()
        .and_then(|index| views.get(index))
        .and_then(Value::as_object)
        .ok_or_else(invalid)?;
    let buffer = view
        .get("buffer")
        .and_then(Value::as_u64)
        .and_then(|index| usize::try_from(index).ok())
        .and_then(|index| buffers.get(index))
        .ok_or_else(invalid)?;
    let offset = view.get("byteOffset").and_then(Value::as_u64).unwrap_or(0);
    let length = view
        .get("byteLength")
        .and_then(Value::as_u64)
        .ok_or_else(invalid)?;
    let start = usize::try_from(offset).map_err(|_| invalid())?;
    let end = offset
        .checked_add(length)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|end| *end <= buffer.len())
        .ok_or_else(invalid)?;
    Ok(&buffer[start..end])
}

fn decode_buffer_uri(uri: &str) -> Result<Vec<u8>, String> {
    let prefix = "data:application/octet-stream;base64,";
    if !uri
        .get(..prefix.len())
        .is_some_and(|value| value.eq_ignore_ascii_case(prefix))
    {
        return Err(invalid());
    }
    decode_data_uri(uri, prefix)
}

fn decode_image_uri(uri: &str) -> Result<(&str, Cow<'_, [u8]>), String> {
    for (prefix, mime) in [
        ("data:image/png;base64,", "image/png"),
        ("data:image/jpeg;base64,", "image/jpeg"),
        ("data:image/webp;base64,", "image/webp"),
    ] {
        if uri
            .get(..prefix.len())
            .is_some_and(|value| value.eq_ignore_ascii_case(prefix))
        {
            return Ok((mime, Cow::Owned(decode_data_uri(uri, prefix)?)));
        }
    }
    Err(invalid())
}

fn decode_data_uri(uri: &str, prefix: &str) -> Result<Vec<u8>, String> {
    if !uri
        .get(..prefix.len())
        .is_some_and(|value| value.eq_ignore_ascii_case(prefix))
    {
        return Err(invalid());
    }
    let encoded = uri.get(prefix.len()..).ok_or_else(invalid)?;
    general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| invalid())
}

fn validate_texture_payload(mime: &str, bytes: &[u8]) -> Result<u64, String> {
    let expected = match mime {
        "image/png" => ImageFormat::Png,
        "image/jpeg" => ImageFormat::Jpeg,
        "image/webp" => ImageFormat::WebP,
        _ => return Err(invalid()),
    };
    if image::guess_format(bytes).ok() != Some(expected) {
        return Err(invalid());
    }
    if expected == ImageFormat::Png && png_is_animated(bytes) {
        return Err(invalid());
    }
    if expected == ImageFormat::WebP && webp_is_animated(bytes) {
        return Err(invalid());
    }
    let (width, height) = ImageReader::with_format(Cursor::new(bytes), expected)
        .into_dimensions()
        .map_err(|_| invalid())?;
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .filter(|pixels| {
            width > 0
                && height > 0
                && width <= MAX_TEXTURE_AXIS
                && height <= MAX_TEXTURE_AXIS
                && *pixels <= MAX_TEXTURE_PIXELS
        })
        .ok_or_else(invalid)?;
    let decoded = ImageReader::with_format(Cursor::new(bytes), expected)
        .decode()
        .map_err(|_| invalid())?;
    if decoded.width() != width || decoded.height() != height {
        return Err(invalid());
    }
    Ok(pixels)
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

fn webp_is_animated(bytes: &[u8]) -> bool {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return false;
    }
    let mut offset = 12_usize;
    while offset.checked_add(8).is_some_and(|end| end <= bytes.len()) {
        let kind = &bytes[offset..offset + 4];
        let length =
            u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap_or_default())
                as usize;
        let Some(payload_end) = offset
            .checked_add(8)
            .and_then(|value| value.checked_add(length))
        else {
            return true;
        };
        if payload_end > bytes.len() {
            return true;
        }
        if kind == b"ANIM" || kind == b"ANMF" {
            return true;
        }
        if kind == b"VP8X" && bytes.get(offset + 8).is_some_and(|flags| flags & 0x02 != 0) {
            return true;
        }
        offset = match payload_end.checked_add(length % 2) {
            Some(offset) => offset,
            None => return true,
        };
    }
    false
}

fn invalid() -> String {
    "The model contains an invalid texture.".to_string()
}

#[cfg(test)]
#[path = "asset_texture_validation_tests.rs"]
mod tests;
