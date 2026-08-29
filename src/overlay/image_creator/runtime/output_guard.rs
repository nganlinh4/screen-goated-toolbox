use std::io::Read as _;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use serde_json::Value;

use super::StartJobRequest;

const MAX_OUTPUT_BYTES: u64 = crate::overlay::generation_history::IMAGE_RESULT_RESERVATION_BYTES;
const MAX_OUTPUT_PIXELS: u64 = 64_000_000;
const MAX_OUTPUT_DIMENSION: u32 = 32_768;
const MAX_OUTPUT_DECODE_BYTES: u64 = MAX_OUTPUT_PIXELS * 4 + 8 * 1024 * 1024;

static OUTPUT_DECODER: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

fn decode_png(path: &std::path::Path) -> Result<(u32, u32), String> {
    let mut header = [0_u8; 24];
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .map_err(|_| "Creation engine returned no image file.".to_string())?;
    if header[..8] != [137, 80, 78, 71, 13, 10, 26, 10] || &header[12..16] != b"IHDR" {
        return Err("Creation engine returned a non-PNG image.".to_string());
    }
    let width = u32::from_be_bytes(header[16..20].try_into().unwrap_or_default());
    let height = u32::from_be_bytes(header[20..24].try_into().unwrap_or_default());
    if width == 0
        || height == 0
        || width > MAX_OUTPUT_DIMENSION
        || height > MAX_OUTPUT_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_OUTPUT_PIXELS
    {
        return Err("Creation engine returned invalid image dimensions.".to_string());
    }

    let _decoder_guard = OUTPUT_DECODER
        .lock()
        .unwrap_or_else(|value| value.into_inner());
    let mut reader = image::ImageReader::open(path)
        .map_err(|_| "Creation engine returned a corrupt PNG image.".to_string())?;
    reader.set_format(image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_OUTPUT_DIMENSION);
    limits.max_image_height = Some(MAX_OUTPUT_DIMENSION);
    limits.max_alloc = Some(MAX_OUTPUT_DECODE_BYTES);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|_| "Creation engine returned a corrupt PNG image.".to_string())?;
    if decoded.width() != width || decoded.height() != height {
        return Err("Creation engine returned inconsistent image dimensions.".to_string());
    }
    Ok((width, height))
}

fn expected_output_path(request: &StartJobRequest) -> Option<PathBuf> {
    let output_name = PathBuf::from(request.output_name.as_deref()?);
    if output_name.file_name() != Some(output_name.as_os_str()) {
        return None;
    }
    Some(PathBuf::from(request.output_dir.as_deref()?).join(output_name))
}

fn is_regular_output_file(path: &std::path::Path) -> bool {
    std::fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && !is_reparse_point(&metadata))
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
    if !is_regular_output_file(&expected) || !is_regular_output_file(&reported) {
        return Err("Creation engine returned an invalid image file.".to_string());
    }
    let expected = std::fs::canonicalize(expected)
        .map_err(|_| "Creation engine returned no image file.".to_string())?;
    let reported = std::fs::canonicalize(reported)
        .map_err(|_| "Creation engine returned no image file.".to_string())?;
    let output_directory = std::fs::canonicalize(
        request
            .output_dir
            .as_deref()
            .ok_or_else(|| "Image output destination is invalid.".to_string())?,
    )
    .map_err(|_| "Image output destination is invalid.".to_string())?;
    let metadata = std::fs::metadata(&expected)
        .map_err(|_| "Creation engine returned no image file.".to_string())?;
    if expected != reported
        || expected.parent() != Some(output_directory.as_path())
        || !metadata.is_file()
        || metadata.len() < 32
        || metadata.len() > MAX_OUTPUT_BYTES
    {
        return Err("Creation engine returned an invalid image file.".to_string());
    }
    let (decoded_width, decoded_height) = decode_png(&expected)?;
    if value.get("mimeType").and_then(Value::as_str) != Some("image/png")
        || value.get("width").and_then(Value::as_u64) != Some(u64::from(decoded_width))
        || value.get("height").and_then(Value::as_u64) != Some(u64::from(decoded_height))
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
        image::RgbaImage::new(2, 3)
            .save_with_format(&expected, image::ImageFormat::Png)
            .expect("write expected output");
        image::RgbaImage::new(2, 3)
            .save_with_format(&conflicting, image::ImageFormat::Png)
            .expect("write conflicting output");
        let request = StartJobRequest {
            image_paths: vec!["reference.png".to_string()],
            image_path: Some("reference.png".to_string()),
            source_descriptors: Vec::new(),
            output_dir: Some(directory.to_string_lossy().to_string()),
            final_output_dir: Some(directory.to_string_lossy().to_string()),
            prompt: "transform".to_string(),
            output_name: Some("Created Image 1.png".to_string()),
            dispatch_id: "dispatch-test".to_string(),
        };
        let valid = json!({
            "outputPath": expected,
            "mimeType": "image/png",
            "width": 2,
            "height": 3,
        });
        assert!(validate_runtime_result(&request, valid).is_ok());
        let invalid = json!({
            "outputPath": conflicting,
            "mimeType": "image/png",
            "width": 2,
            "height": 3,
        });
        assert!(validate_runtime_result(&request, invalid).is_err());
        std::fs::write(&expected, b"\x89PNG\r\n\x1a\nnot-a-real-png").unwrap();
        let corrupt = json!({
            "outputPath": expected,
            "mimeType": "image/png",
            "width": 2,
            "height": 3,
        });
        assert!(validate_runtime_result(&request, corrupt).is_err());

        std::fs::remove_file(expected).expect("remove expected output");
        std::fs::remove_file(conflicting).expect("remove conflicting output");
        std::fs::remove_dir(directory).expect("remove test directory");
    }

    #[test]
    fn png_dimensions_are_bounded_per_axis_before_decode() {
        let path = test_directory("dimensions").with_extension("png");
        let mut header = Vec::from(&b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"[..]);
        header.extend_from_slice(&(MAX_OUTPUT_DIMENSION + 1).to_be_bytes());
        header.extend_from_slice(&1_u32.to_be_bytes());
        std::fs::write(&path, header).unwrap();

        assert!(decode_png(&path).is_err());

        std::fs::remove_file(path).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn result_symlink_cannot_escape_the_selected_directory() {
        use std::os::windows::fs::symlink_file;

        let directory = test_directory("symlink-root");
        let outside = test_directory("symlink-outside");
        std::fs::create_dir(&directory).unwrap();
        std::fs::create_dir(&outside).unwrap();
        let external = outside.join("external.png");
        image::RgbaImage::new(2, 3)
            .save_with_format(&external, image::ImageFormat::Png)
            .unwrap();
        let linked = directory.join("Created Image linked.png");
        if symlink_file(&external, &linked).is_err() {
            std::fs::remove_dir_all(directory).unwrap();
            std::fs::remove_dir_all(outside).unwrap();
            return;
        }
        let request = StartJobRequest {
            image_paths: Vec::new(),
            image_path: None,
            source_descriptors: Vec::new(),
            output_dir: Some(directory.to_string_lossy().to_string()),
            final_output_dir: Some(directory.to_string_lossy().to_string()),
            prompt: "create".to_string(),
            output_name: Some("Created Image linked.png".to_string()),
            dispatch_id: "dispatch-test".to_string(),
        };
        let result = json!({
            "outputPath": linked,
            "mimeType": "image/png",
            "width": 2,
            "height": 3,
        });

        assert!(validate_runtime_result(&request, result).is_err());

        let internal = directory.join("internal.png");
        image::RgbaImage::new(2, 3)
            .save_with_format(&internal, image::ImageFormat::Png)
            .unwrap();
        let same_directory_link = directory.join("Created Image same directory.png");
        symlink_file(&internal, &same_directory_link).unwrap();
        let same_directory_request = StartJobRequest {
            output_name: Some("Created Image same directory.png".to_string()),
            ..request
        };
        let same_directory_result = json!({
            "outputPath": same_directory_link,
            "mimeType": "image/png",
            "width": 2,
            "height": 3,
        });
        assert!(validate_runtime_result(&same_directory_request, same_directory_result).is_err());

        std::fs::remove_file(linked).unwrap();
        std::fs::remove_file(same_directory_link).unwrap();
        std::fs::remove_file(internal).unwrap();
        std::fs::remove_dir_all(directory).unwrap();
        std::fs::remove_dir_all(outside).unwrap();
    }
}
