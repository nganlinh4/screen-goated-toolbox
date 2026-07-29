use std::io::{Cursor, Write as _};
use std::sync::{LazyLock, Mutex};

use image::ImageReader;
use sha2::{Digest as _, Sha256};

use super::{SourceDescriptor, file_sha256, preview_root, record_presentations, validate_sources};

const MAX_PREVIEW_EDGE: u32 = 512;
const MAX_PREVIEW_BYTES: usize = 2 * 1024 * 1024;
pub(super) static PRESENTATION_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

pub(crate) fn presentation_path(descriptors: &[SourceDescriptor]) -> Result<String, String> {
    Ok(presentation_paths(descriptors)?
        .into_iter()
        .next()
        .unwrap_or_default())
}

pub(crate) fn presentation_paths(descriptors: &[SourceDescriptor]) -> Result<Vec<String>, String> {
    if descriptors.is_empty() {
        return Ok(Vec::new());
    }
    validate_sources(descriptors)?;
    let _guard = PRESENTATION_LOCK
        .lock()
        .map_err(|_| "Creation preview is unavailable.".to_string())?;
    let mut paths = Vec::with_capacity(descriptors.len());
    for descriptor in descriptors {
        paths.push(prepare_one(descriptor)?);
        record_presentations(descriptors, &paths)?;
    }
    Ok(paths)
}

fn prepare_one(source: &SourceDescriptor) -> Result<String, String> {
    let mut reader = ImageReader::open(&source.path)
        .map_err(|_| "Creation preview could not be prepared.".to_string())?
        .with_guessed_format()
        .map_err(|_| "Creation preview could not be prepared.".to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(crate::overlay::creation_source::MAX_SOURCE_DIMENSION);
    limits.max_image_height = Some(crate::overlay::creation_source::MAX_SOURCE_DIMENSION);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|_| "Creation preview could not be prepared.".to_string())?
        .thumbnail(MAX_PREVIEW_EDGE, MAX_PREVIEW_EDGE);
    let mut encoded = Cursor::new(Vec::new());
    image
        .write_to(&mut encoded, image::ImageFormat::Png)
        .map_err(|_| "Creation preview could not be prepared.".to_string())?;
    let encoded = encoded.into_inner();
    if encoded.is_empty() || encoded.len() > MAX_PREVIEW_BYTES {
        return Err("Creation preview exceeds its storage limit.".to_string());
    }
    let digest = format!("{:x}", Sha256::digest(&encoded));
    let path = preview_root()?.join(format!("{digest}.png"));
    if let Ok(metadata) = std::fs::symlink_metadata(&path) {
        if metadata.file_type().is_file()
            && metadata.len() == encoded.len() as u64
            && file_sha256(&path)? == digest
        {
            return Ok(path.to_string_lossy().to_string());
        }
        return Err("Creation preview conflicts with saved state.".to_string());
    }
    let temporary = preview_root()?.join(format!(
        ".sgt-preview-{}.tmp",
        crate::overlay::creation_identity::random_id("asset-")?
    ));
    let write_result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| "Creation preview could not be saved.".to_string())?;
        file.write_all(&encoded)
            .and_then(|_| file.flush())
            .and_then(|_| file.sync_all())
            .map_err(|_| "Creation preview could not be saved.".to_string())?;
        drop(file);
        let owned = crate::overlay::creation_delivery::publication::lock_owned_path(
            &temporary, None, false, true,
        )?;
        crate::overlay::creation_delivery::publication::rename_owned_no_replace(
            &owned, &temporary, &path,
        )
    })();
    if let Err(error) = write_result {
        if let Ok(owned) = crate::overlay::creation_delivery::publication::lock_owned_path(
            &temporary, None, false, true,
        ) {
            let _ =
                crate::overlay::creation_delivery::publication::delete_owned(&owned, &temporary);
        }
        if std::fs::symlink_metadata(&path).is_ok()
            && file_sha256(&path).is_ok_and(|saved| saved == digest)
        {
            return Ok(path.to_string_lossy().to_string());
        }
        return Err(error);
    }
    Ok(path.to_string_lossy().to_string())
}
