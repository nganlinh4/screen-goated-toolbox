//! Shared, owner-bound native pickers for Creation tools.
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use windows::Win32::Foundation::HWND;

#[cfg(windows)]
mod native;

pub(crate) const MAX_BATCH_IMAGES: usize = 100;
pub(crate) const MAX_BATCH_BYTES: u64 = 512 * 1024 * 1024;
const MAX_IMAGE_BYTES: u64 = 25 * 1024 * 1024;

#[derive(Clone, Copy)]
enum Selection {
    Images,
    Directory,
}

static ACTIVE: LazyLock<Mutex<HashSet<isize>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
struct DialogLease(isize);
impl DialogLease {
    fn acquire(owner: isize) -> Result<Self, String> {
        if owner == 0
            || !ACTIVE
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .insert(owner)
        {
            return Err("A creation picker is already open or its window is unavailable.".into());
        }
        Ok(Self(owner))
    }
}
impl Drop for DialogLease {
    fn drop(&mut self) {
        ACTIVE
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.0);
    }
}

fn select(owner: HWND, selection: Selection) -> Result<Vec<PathBuf>, String> {
    let _lease = DialogLease::acquire(owner.0 as isize)?;
    #[cfg(windows)]
    {
        native::select(owner, selection).map_err(|error| error.to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = selection;
        Ok(Vec::new())
    }
}

pub(crate) fn pick_images_dialog(owner: HWND) -> Result<Vec<PathBuf>, String> {
    admit_image_paths(
        select(owner, Selection::Images)?,
        MAX_BATCH_IMAGES,
        MAX_BATCH_BYTES,
    )
}
pub(crate) fn pick_output_dir_dialog(owner: HWND) -> Result<Option<PathBuf>, String> {
    select(owner, Selection::Directory).map(|mut paths| paths.pop())
}

pub(crate) fn is_supported_image_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "webp"
            )
        })
}

pub(crate) fn admit_image_paths(
    paths: Vec<PathBuf>,
    max_count: usize,
    max_total_bytes: u64,
) -> Result<Vec<PathBuf>, String> {
    if paths.len() > max_count {
        return Err(format!("Select no more than {max_count} images at once."));
    }
    let mut total = 0_u64;
    for path in &paths {
        if !is_supported_image_path(path) {
            return Err("Images must be PNG, JPEG, or WebP.".to_string());
        }
        let metadata =
            std::fs::metadata(path).map_err(|_| "A selected image is unavailable.".to_string())?;
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_IMAGE_BYTES {
            return Err("A selected image is empty or exceeds 25 MiB.".to_string());
        }
        total = total
            .checked_add(metadata.len())
            .filter(|total| *total <= max_total_bytes)
            .ok_or_else(|| "The selected images are too large in total.".to_string())?;
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modal_ownership_rejects_nested_pickers_and_releases_after_error() {
        let failure = || -> Result<(), String> {
            let _lease = DialogLease::acquire(1001)?;
            assert!(DialogLease::acquire(1001).is_err());
            assert!(DialogLease::acquire(1002).is_ok());
            Err("Selection unavailable".into())
        };
        assert!(failure().is_err());
        assert!(DialogLease::acquire(1001).is_ok());
        assert!(DialogLease::acquire(0).is_err());
    }

    #[test]
    fn admission_rejects_the_101st_batch_item_before_io() {
        let paths = (0..=MAX_BATCH_IMAGES)
            .map(|index| PathBuf::from(format!("{index}.png")))
            .collect();
        assert!(admit_image_paths(paths, MAX_BATCH_IMAGES, MAX_BATCH_BYTES).is_err());
    }

    #[test]
    fn admission_rejects_unsupported_extensions() {
        assert!(!is_supported_image_path(
            PathBuf::from("image.bmp").as_path()
        ));
        assert!(!is_supported_image_path(
            PathBuf::from("image.svg").as_path()
        ));
        assert!(is_supported_image_path(
            PathBuf::from("image.WEBP").as_path()
        ));
    }

    #[test]
    fn admission_counts_duplicate_occurrences_toward_the_aggregate_limit() {
        let path = std::env::temp_dir().join(format!(
            "sgt-admission-aggregate-{}.png",
            std::process::id()
        ));
        std::fs::File::create(&path)
            .and_then(|file| file.set_len(MAX_IMAGE_BYTES))
            .unwrap();
        assert!(
            admit_image_paths(
                vec![
                    path.clone(),
                    path.clone(),
                    path.clone(),
                    path.clone(),
                    path.clone(),
                ],
                MAX_BATCH_IMAGES,
                MAX_IMAGE_BYTES * 4,
            )
            .is_err()
        );
        std::fs::remove_file(path).unwrap();
    }
}
