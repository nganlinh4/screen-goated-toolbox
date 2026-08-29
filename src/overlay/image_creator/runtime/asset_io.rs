use std::path::PathBuf;

pub(super) const MAX_TOTAL_REFERENCE_BYTES: u64 =
    crate::overlay::generation_history::IMAGE_REFERENCE_RESERVATION_BYTES;

pub(super) fn inspect_reference(
    path: &str,
) -> Result<crate::overlay::creation_source::InspectedImage, String> {
    crate::overlay::creation_source::inspect_image(PathBuf::from(path))
}

pub(in crate::overlay::image_creator) fn open_output(
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
