use std::io;
#[cfg(test)]
use std::io::Write as _;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::Ordering;

#[cfg(test)]
use super::TEMP_SEQUENCE;

/// ReplaceFileW-compatible outcome codes. Identity-bound publication reports
/// the same codes so retry and uncertainty classification stay unchanged:
/// 1175 means nothing moved; 1176 means the preimage already sits in backup.
pub(super) const UNABLE_TO_REMOVE_REPLACED: i32 = 1175;
pub(super) const UNABLE_TO_MOVE_REPLACEMENT: i32 = 1176;
#[cfg(test)]
const UNABLE_TO_MOVE_REPLACEMENT_2: i32 = 1177;
const REPLACE_ATTEMPTS: usize = 6;

pub(super) fn has_recovery_candidate(path: &Path) -> Result<bool, String> {
    let parent = path
        .parent()
        .ok_or("Creation recovery folder is unavailable.")?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Creation recovery filename is invalid.")?;
    let prefix = format!(".{name}.");
    let entries = match std::fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err("Creation recovery folder is unavailable.".to_string()),
    };
    for entry in entries {
        let entry = entry.map_err(|_| "Creation recovery folder is unavailable.".to_string())?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(parts) = name
            .strip_prefix(&prefix)
            .and_then(|name| name.strip_suffix(".tmp"))
        else {
            continue;
        };
        if let Some((pid, sequence)) = parts.split_once('-')
            && !pid.is_empty()
            && !sequence.is_empty()
            && pid.bytes().all(|byte| byte.is_ascii_digit())
            && sequence.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn write(path: &Path, bytes: &[u8]) -> Result<(), super::RecordError> {
    super::transaction::write(path, bytes)
}

#[cfg(test)]
fn write_using(
    path: &Path,
    bytes: &[u8],
    mut replace: impl FnMut(&Path, &Path) -> io::Result<()>,
) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or("Creation recovery folder is unavailable.")?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("Creation recovery folder could not be created: {error}"))?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("Creation recovery filename is invalid.")?;
    let (temporary, mut file) = allocate_temporary(parent, name, || {
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    })
    .map_err(|error| format!("Creation recovery state could not be saved: {error}"))?;
    let prepared = file
        .write_all(bytes)
        .and_then(|()| file.flush())
        .and_then(|()| file.sync_all());
    drop(file);
    let result = prepared.and_then(|()| retry_replace(|| replace(path, &temporary)));
    if result
        .as_ref()
        .is_err_and(|error| !may_have_moved_original(error))
    {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(|error| format!("Creation recovery state could not be saved: {error}"))
}

pub(super) fn allocate_temporary(
    parent: &Path,
    name: &str,
    mut next: impl FnMut() -> u64,
) -> io::Result<(PathBuf, std::fs::File)> {
    for _ in 0..128 {
        let temporary = parent.join(format!(".{name}.{}-{}.tmp", std::process::id(), next()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "Creation recovery temporary filename is unavailable.",
    ))
}

#[cfg(test)]
fn may_have_moved_original(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(UNABLE_TO_MOVE_REPLACEMENT | UNABLE_TO_MOVE_REPLACEMENT_2)
    )
}

pub(super) fn retry_replace(mut replace: impl FnMut() -> io::Result<()>) -> io::Result<()> {
    for attempt in 0..REPLACE_ATTEMPTS {
        match replace() {
            Err(error)
                if error.raw_os_error() == Some(UNABLE_TO_REMOVE_REPLACED)
                    && attempt + 1 < REPLACE_ATTEMPTS =>
            {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            result => return result,
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_only_safe_replacement_failure() {
        let mut attempts = 0;
        retry_replace(|| {
            attempts += 1;
            if attempts < 3 {
                Err(io::Error::from_raw_os_error(1175))
            } else {
                Ok(())
            }
        })
        .unwrap();
        assert_eq!(attempts, 3);
        for code in [1176, 1177, 5] {
            let mut calls = 0;
            let error = retry_replace(|| {
                calls += 1;
                Err(io::Error::from_raw_os_error(code))
            })
            .unwrap_err();
            assert_eq!(calls, 1);
            assert_eq!(
                may_have_moved_original(&error),
                code == 1176 || code == 1177
            );
        }
    }

    #[test]
    fn missing_journal_with_recovery_candidate_fails_closed() {
        let root = std::env::temp_dir().join(format!(
            "sgt-intent-recovery-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("journal.json");
        let candidate = root.join(".journal.json.42-7.tmp");
        std::fs::write(&candidate, b"saved recovery copy").unwrap();
        assert!(super::super::read_store(&path).is_err());
        assert_eq!(std::fs::read(&candidate).unwrap(), b"saved recovery copy");
        std::fs::remove_file(candidate).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn ambiguous_replacement_failures_keep_the_only_new_copy() {
        for code in [1176, 1177] {
            let root = std::env::temp_dir().join(format!(
                "sgt-intent-replace-{}-{}",
                std::process::id(),
                TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&root).unwrap();
            let path = root.join("journal.json");
            std::fs::write(&path, b"old copy").unwrap();
            let result = write_using(&path, b"new copy", |path, _| {
                std::fs::remove_file(path)?;
                Err(io::Error::from_raw_os_error(code))
            });
            assert!(result.is_err());
            assert!(super::super::read_store(&path).is_err());
            let files = std::fs::read_dir(&root)
                .unwrap()
                .map(|item| item.unwrap().path())
                .collect::<Vec<_>>();
            assert_eq!(files.len(), 1);
            assert_eq!(std::fs::read(&files[0]).unwrap(), b"new copy");
            std::fs::remove_file(&files[0]).unwrap();
            std::fs::remove_dir(root).unwrap();
        }
    }

    #[test]
    fn temporary_name_collision_preserves_existing_file() {
        let root = std::env::temp_dir().join(format!(
            "sgt-intent-collision-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let collision = root.join(format!(".journal.json.{}-7.tmp", std::process::id()));
        std::fs::write(&collision, b"existing recovery copy").unwrap();
        let mut sequence = [7, 8].into_iter();
        let (created, file) =
            allocate_temporary(&root, "journal.json", || sequence.next().unwrap()).unwrap();
        drop(file);
        assert_eq!(
            std::fs::read(&collision).unwrap(),
            b"existing recovery copy"
        );
        std::fs::remove_file(created).unwrap();
        std::fs::remove_file(collision).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
