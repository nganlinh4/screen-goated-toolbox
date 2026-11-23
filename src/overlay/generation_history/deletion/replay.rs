//! Crash-safe replay of a durable result deletion.
//!
//! Every member first moves to a hidden name derived from the entry, and no
//! member is deleted while another one is still visible. After a crash each
//! member is therefore visible and exact, hidden and exact, or gone. If a
//! visible member changed before the group was hidden, hidden members move
//! back and the entry returns to history, so a pair is never half deleted.

use sha2::{Digest as _, Sha256};

use super::*;

#[derive(Clone, Copy)]
pub(in crate::overlay::generation_history) enum Role {
    Primary,
    Companion,
}

pub(in crate::overlay::generation_history) fn quarantine_path(
    entry: &ResultHistoryEntry,
    original: &Path,
    role: Role,
) -> Option<PathBuf> {
    let mut digest = Sha256::new();
    digest.update(entry.tool.as_bytes());
    digest.update([0]);
    digest.update(entry.id.as_bytes());
    let key = format!("{:x}", digest.finalize());
    let role = match role {
        Role::Primary => "primary",
        Role::Companion => "companion",
    };
    Some(
        original
            .parent()?
            .join(format!(".sgt-history-delete-{}-{role}.tmp", &key[..32])),
    )
}

pub(super) enum Replay {
    /// Keep the journal; a later pass retries from the same durable state.
    Retry,
    Deleted,
    /// The deletion was abandoned; the entry returns when its primary exists.
    Restored(Option<Box<ResultHistoryEntry>>),
}

struct Member {
    original: PathBuf,
    quarantine: PathBuf,
    size: u64,
    sha256: String,
    identity: String,
}

pub(in crate::overlay::generation_history) enum Probe {
    Exact(std::fs::File),
    Absent,
    Different,
    Unavailable,
}

enum Location {
    Visible(std::fs::File),
    Hidden(std::fs::File),
    Gone,
    Changed,
}

/// Classify `path` against one exact recorded artifact without following links.
pub(in crate::overlay::generation_history) fn probe(
    path: &Path,
    size: u64,
    sha256: &str,
    identity: &str,
) -> Probe {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return match path.parent().map(std::fs::metadata) {
                Some(Ok(metadata)) if metadata.is_dir() => Probe::Absent,
                _ => Probe::Unavailable,
            };
        }
        Err(_) => return Probe::Unavailable,
        Ok(metadata) if !metadata.file_type().is_file() => return Probe::Different,
        Ok(_) => {}
    }
    let Ok(file) =
        crate::overlay::creation_delivery::publication::lock_owned_path(path, None, false, true)
    else {
        return Probe::Unavailable;
    };
    match inspect_delivery_artifact(&path.to_string_lossy()) {
        Ok(artifact)
            if artifact.size_bytes == size
                && artifact.sha256 == sha256
                && artifact.file_identity == identity =>
        {
            Probe::Exact(file)
        }
        Ok(_) => Probe::Different,
        Err(_) => Probe::Unavailable,
    }
}

/// `None` means the state cannot be classified safely right now.
fn locate(member: &Member) -> Option<Location> {
    let check = |path: &Path| probe(path, member.size, &member.sha256, &member.identity);
    match check(&member.quarantine) {
        Probe::Exact(file) => return Some(Location::Hidden(file)),
        Probe::Absent => {}
        // Unknown bytes under the reserved name are never adopted or replaced.
        Probe::Different | Probe::Unavailable => return None,
    }
    match check(&member.original) {
        Probe::Exact(file) => Some(Location::Visible(file)),
        Probe::Absent => Some(Location::Gone),
        Probe::Different => Some(Location::Changed),
        Probe::Unavailable => None,
    }
}

fn members(entry: &ResultHistoryEntry, primary_identity: &str) -> Option<Vec<Member>> {
    let side = companion::checked_from_entry(entry).ok()?;
    let primary = PathBuf::from(&entry.output_path);
    let mut members = vec![Member {
        quarantine: quarantine_path(entry, &primary, Role::Primary)?,
        original: primary,
        size: entry.artifact_size_bytes,
        sha256: entry.artifact_sha256.clone(),
        identity: primary_identity.to_string(),
    }];
    if let Some(side) = side {
        let original = PathBuf::from(&side.path);
        members.push(Member {
            quarantine: quarantine_path(entry, &original, Role::Companion)?,
            original,
            size: side.size_bytes,
            sha256: side.sha256,
            identity: side.file_identity,
        });
    }
    members
        .iter()
        .all(|member| {
            member.original.is_absolute()
                && member.sha256.len() == 64
                && crate::overlay::creation_file_identity::valid(&member.identity)
        })
        .then_some(members)
}

pub(super) fn execute(pending: &PendingDelete) -> Replay {
    let entry = &pending.entry;
    if validate_tool(&entry.tool).is_err() || entry.output_path.len() > MAX_HISTORY_PATH_BYTES {
        return Replay::Retry;
    }
    let Some(members) = members(entry, &pending.artifact_file_identity) else {
        return Replay::Retry;
    };
    let Some(mut located) = members.iter().map(locate).collect::<Option<Vec<_>>>() else {
        return Replay::Retry;
    };
    if located
        .iter()
        .any(|location| matches!(location, Location::Changed))
    {
        return roll_back(entry, &members, located);
    }
    for (member, location) in members.iter().zip(located.iter_mut()) {
        *location = match std::mem::replace(location, Location::Gone) {
            Location::Visible(file) => {
                if crate::overlay::creation_delivery::publication::rename_owned_no_replace(
                    &file,
                    &member.original,
                    &member.quarantine,
                )
                .is_err()
                {
                    return Replay::Retry;
                }
                Location::Hidden(file)
            }
            other => other,
        };
    }
    for (member, location) in members.iter().zip(located) {
        if let Location::Hidden(file) = location
            && crate::overlay::creation_delivery::publication::delete_owned(
                &file,
                &member.quarantine,
            )
            .is_err()
        {
            return Replay::Retry;
        }
    }
    Replay::Deleted
}

fn roll_back(entry: &ResultHistoryEntry, members: &[Member], located: Vec<Location>) -> Replay {
    let mut companion_gone = false;
    let mut primary_gone = false;
    for (index, (member, location)) in members.iter().zip(located).enumerate() {
        match location {
            Location::Hidden(file) => {
                if crate::overlay::creation_delivery::publication::rename_owned_no_replace(
                    &file,
                    &member.quarantine,
                    &member.original,
                )
                .is_err()
                {
                    return Replay::Retry;
                }
            }
            Location::Gone if index == 0 => primary_gone = true,
            Location::Gone => companion_gone = true,
            Location::Visible(_) | Location::Changed => {}
        }
    }
    if primary_gone {
        return Replay::Restored(None);
    }
    let mut restored = entry.clone();
    if companion_gone && let Some(object) = restored.metadata.as_object_mut() {
        object.remove("download");
        restored.companion_origin_path = None;
    }
    Replay::Restored(Some(Box::new(restored)))
}
