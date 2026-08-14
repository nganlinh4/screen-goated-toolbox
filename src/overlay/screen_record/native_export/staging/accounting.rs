use std::mem::size_of;

use super::super::config::{BakedCameraFrame, BakedCursorFrame, BakedWebcamFrame, OverlayFrame};
use super::super::overlay_frames::OverlayAtlasMetadata;
use super::CursorSlotOverride;

pub(super) const MAX_STAGED_JOB_BYTES: usize = 512 * 1024 * 1024;
pub(super) const MAX_STAGED_SESSION_BYTES: usize = 768 * 1024 * 1024;
pub(super) const MAX_STAGED_TOTAL_BYTES: usize = 1024 * 1024 * 1024;
const MAX_LEGACY_STAGED_BYTES: usize = MAX_STAGED_JOB_BYTES;

#[derive(Clone, Copy)]
pub(super) struct ByteUpdate {
    removed: usize,
    added: usize,
}

impl ByteUpdate {
    pub(super) const fn append(added: usize) -> Self {
        Self { removed: 0, added }
    }

    pub(super) const fn replace(removed: usize, added: usize) -> Self {
        Self { removed, added }
    }
}

fn apply(current: usize, update: ByteUpdate) -> Result<usize, String> {
    current
        .checked_sub(update.removed)
        .and_then(|bytes| bytes.checked_add(update.added))
        .ok_or_else(|| "Staging byte accounting overflowed".to_string())
}

pub(super) fn project_legacy(
    current: usize,
    current_total: usize,
    update: ByteUpdate,
) -> Result<usize, String> {
    let projected = apply(current, update)?;
    if projected > MAX_LEGACY_STAGED_BYTES {
        return Err("Staged export exceeds the 512 MiB byte limit".to_string());
    }
    let total = apply(current_total, update)?;
    if total > MAX_STAGED_TOTAL_BYTES {
        return Err("All staged export sessions exceed the 1 GiB byte limit".to_string());
    }
    Ok(projected)
}

pub(super) fn project_scoped(
    current_job: usize,
    current_session: usize,
    current_total: usize,
    update: ByteUpdate,
) -> Result<usize, String> {
    let job = apply(current_job, update)?;
    let session = apply(current_session, update)?;
    let total = apply(current_total, update)?;
    if job > MAX_STAGED_JOB_BYTES {
        return Err("Staged export job exceeds the 512 MiB byte limit".to_string());
    }
    if session > MAX_STAGED_SESSION_BYTES {
        return Err("Staged export session exceeds the 768 MiB byte limit".to_string());
    }
    if total > MAX_STAGED_TOTAL_BYTES {
        return Err("All staged export sessions exceed the 1 GiB byte limit".to_string());
    }
    Ok(job)
}

fn fixed_slice_bytes<T>(len: usize) -> Result<usize, String> {
    len.checked_mul(size_of::<T>())
        .ok_or_else(|| "Staging byte count overflowed".to_string())
}

fn slice_bytes<T>(values: &[T]) -> Result<usize, String> {
    fixed_slice_bytes::<T>(values.len())
}

fn checked_sum(values: impl IntoIterator<Item = usize>) -> Result<usize, String> {
    values.into_iter().try_fold(0usize, |total, value| {
        total
            .checked_add(value)
            .ok_or_else(|| "Staging byte count overflowed".to_string())
    })
}

pub(super) fn camera_frames(frames: &[BakedCameraFrame]) -> Result<usize, String> {
    fixed_slice_bytes::<BakedCameraFrame>(frames.len())
}

pub(super) fn cursor_frames(frames: &[BakedCursorFrame]) -> Result<usize, String> {
    checked_sum([
        fixed_slice_bytes::<BakedCursorFrame>(frames.len())?,
        checked_sum(frames.iter().map(|frame| frame.cursor_type.len()))?,
    ])
}

pub(super) fn webcam_frames(frames: &[BakedWebcamFrame]) -> Result<usize, String> {
    fixed_slice_bytes::<BakedWebcamFrame>(frames.len())
}

pub(super) fn cursor_overrides(overrides: &[CursorSlotOverride]) -> Result<usize, String> {
    checked_sum([
        fixed_slice_bytes::<CursorSlotOverride>(overrides.len())?,
        checked_sum(overrides.iter().map(|entry| entry.rgba.len()))?,
    ])
}

pub(super) fn overlay_frames(frames: &[OverlayFrame]) -> Result<usize, String> {
    let quad_bytes = frames.iter().try_fold(0usize, |total, frame| {
        total
            .checked_add(slice_bytes(&frame.quads)?)
            .ok_or_else(|| "Staging byte count overflowed".to_string())
    })?;
    checked_sum([fixed_slice_bytes::<OverlayFrame>(frames.len())?, quad_bytes])
}

pub(super) fn overlay_metadata(metadata: &OverlayAtlasMetadata) -> Result<usize, String> {
    let fixed = checked_sum([
        size_of::<OverlayAtlasMetadata>(),
        slice_bytes(&metadata.text_entries)?,
        slice_bytes(&metadata.keystroke_entries)?,
        slice_bytes(&metadata.visibility_segments)?,
        slice_bytes(&metadata.display_events)?,
        slice_bytes(&metadata.keyboard_start_times)?,
        slice_bytes(&metadata.keyboard_indices)?,
        slice_bytes(&metadata.mouse_start_times)?,
        slice_bytes(&metadata.mouse_indices)?,
        slice_bytes(&metadata.event_slots)?,
        slice_bytes(&metadata.event_identities)?,
        slice_bytes(&metadata.keyboard_slot_representative_widths)?,
        slice_bytes(&metadata.mouse_slot_representative_widths)?,
    ])?;
    let strings = checked_sum(
        std::iter::once(metadata.keystroke_mode.len())
            .chain(
                metadata
                    .text_entries
                    .iter()
                    .map(|entry| entry.animation_preset.len()),
            )
            .chain(
                metadata
                    .keystroke_entries
                    .iter()
                    .map(|entry| entry.unique_key.len()),
            )
            .chain(
                metadata
                    .display_events
                    .iter()
                    .flat_map(|event| [event.unique_key.len(), event.event_type.len()]),
            )
            .chain(metadata.event_identities.iter().map(String::len)),
    )?;
    checked_sum([fixed, strings])
}

#[cfg(test)]
mod tests {
    use super::{
        ByteUpdate, MAX_STAGED_JOB_BYTES, MAX_STAGED_SESSION_BYTES, MAX_STAGED_TOTAL_BYTES,
        project_scoped,
    };

    #[test]
    fn aggregate_limits_count_other_jobs_and_sessions() {
        assert!(
            project_scoped(
                MAX_STAGED_JOB_BYTES - 1,
                MAX_STAGED_JOB_BYTES - 1,
                MAX_STAGED_JOB_BYTES - 1,
                ByteUpdate::append(2),
            )
            .is_err()
        );
        assert!(
            project_scoped(
                0,
                MAX_STAGED_SESSION_BYTES - 1,
                MAX_STAGED_SESSION_BYTES - 1,
                ByteUpdate::append(2),
            )
            .is_err()
        );
        assert!(project_scoped(0, 0, MAX_STAGED_TOTAL_BYTES - 1, ByteUpdate::append(2),).is_err());
    }

    #[test]
    fn replacement_releases_old_bytes_before_applying_new_bytes() {
        let projection = project_scoped(
            MAX_STAGED_JOB_BYTES,
            MAX_STAGED_JOB_BYTES,
            MAX_STAGED_JOB_BYTES,
            ByteUpdate::replace(MAX_STAGED_JOB_BYTES, 16),
        )
        .expect("replacement should reclaim its previous payload");
        assert_eq!(projection, 16);
    }
}
