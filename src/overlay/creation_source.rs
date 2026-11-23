use std::io::{Read as _, Seek as _};
use std::path::{Path, PathBuf};

use image::{ImageFormat, ImageReader};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

pub const MAX_SOURCE_BYTES: u64 = 25 * 1024 * 1024;
pub const MAX_SOURCE_DIMENSION: u32 = 32_768;
pub const MAX_SOURCE_PIXELS: u64 = 64_000_000;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDescriptor {
    pub path: String,
    pub size_bytes: u64,
    pub sha256: String,
}

pub struct InspectedImage {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub sha256: String,
    format: ImageFormat,
}

impl InspectedImage {
    pub fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            path: self.path.to_string_lossy().to_string(),
            size_bytes: self.size_bytes,
            sha256: self.sha256.clone(),
        }
    }

    pub(crate) fn canonical_extension(&self) -> &'static str {
        match self.format {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpg",
            ImageFormat::WebP => "webp",
            _ => unreachable!("unsupported formats are rejected during inspection"),
        }
    }
}

pub fn inspect_image(path: impl AsRef<Path>) -> Result<InspectedImage, String> {
    let path = std::fs::canonicalize(path.as_ref())
        .map_err(|error| format!("Could not open source image: {error}"))?;
    crate::overlay::creation_intent_journal::validate_persisted_path(&path)?;
    let mut file = std::fs::File::open(&path)
        .map_err(|error| format!("Could not open {}: {error}", path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("Could not inspect {}: {error}", path.display()))?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_SOURCE_BYTES {
        return Err("Source image is empty or exceeds 25 MiB.".to_string());
    }
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .by_ref()
            .take(MAX_SOURCE_BYTES + 1 - total.min(MAX_SOURCE_BYTES + 1))
            .read(&mut buffer)
            .map_err(|_| "Could not read the source image.".to_string())?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > MAX_SOURCE_BYTES {
            return Err("Source image is empty or exceeds 25 MiB.".to_string());
        }
        hasher.update(&buffer[..read]);
    }
    if total != metadata.len() {
        return Err("Source image changed while it was being inspected.".to_string());
    }
    file.rewind()
        .map_err(|_| "Could not read the source image header.".to_string())?;

    let reader = ImageReader::new(std::io::BufReader::new(file))
        .with_guessed_format()
        .map_err(|_| "Could not identify the source image format.".to_string())?;
    let Some(format @ (ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP)) = reader.format()
    else {
        return Err("Source image must be PNG, JPEG, or WebP.".to_string());
    };
    let (width, height) = reader
        .into_dimensions()
        .map_err(|_| "Could not read the source image dimensions.".to_string())?;
    let pixels = u64::from(width) * u64::from(height);
    if width == 0
        || height == 0
        || width > MAX_SOURCE_DIMENSION
        || height > MAX_SOURCE_DIMENSION
        || pixels > MAX_SOURCE_PIXELS
    {
        return Err("Source image exceeds the supported dimensions.".to_string());
    }
    Ok(InspectedImage {
        path,
        size_bytes: metadata.len(),
        sha256: format!("{:x}", hasher.finalize()),
        format,
    })
}

pub fn revalidate_source(descriptor: &SourceDescriptor) -> Result<InspectedImage, String> {
    let inspected = inspect_image(&descriptor.path)?;
    if inspected.size_bytes != descriptor.size_bytes
        || inspected.sha256 != descriptor.sha256
        || inspected.path.to_string_lossy() != descriptor.path
    {
        return Err("A source image changed after it was selected.".to_string());
    }
    Ok(inspected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_supported_image_headers_and_rejects_arbitrary_bytes() {
        assert_eq!(MAX_SOURCE_PIXELS, 64_000_000);
        let root = std::env::temp_dir().join(format!(
            "sgt-source-inspection-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let valid = root.join("valid.png");
        let invalid = root.join("invalid.png");
        image::RgbaImage::new(3, 2).save(&valid).unwrap();
        std::fs::write(&invalid, b"not an image").unwrap();

        let inspected = inspect_image(&valid).unwrap();
        assert_eq!(
            inspected.size_bytes,
            std::fs::metadata(&valid).unwrap().len()
        );
        revalidate_source(&inspected.descriptor()).unwrap();
        let mut changed = inspected.descriptor();
        changed.sha256 = "0".repeat(64);
        assert!(revalidate_source(&changed).is_err());
        assert!(inspect_image(&invalid).is_err());

        std::fs::remove_dir_all(root).unwrap();
    }
}
