use std::collections::HashMap;
use std::path::{Component, Path};

pub(super) fn volume_key(path: &Path) -> Result<String, String> {
    let path = crate::overlay::creation_path::normalized_absolute(path)?;
    match path.components().next() {
        Some(Component::Prefix(prefix)) => Ok(prefix.as_os_str().to_string_lossy().to_string()),
        _ => Err("Creation storage location is unavailable.".to_string()),
    }
}

pub(super) fn volume_bytes(
    reservations: &HashMap<String, u64>,
    volume: &str,
) -> Result<u64, String> {
    reservations.iter().try_fold(0_u64, |sum, (key, bytes)| {
        if crate::overlay::creation_path::same(key, volume)? {
            sum.checked_add(*bytes)
                .ok_or_else(|| "Creation storage reservation is too large.".to_string())
        } else {
            Ok(sum)
        }
    })
}

pub(super) fn volume_capacity(path: &Path) -> Result<(u64, u64), String> {
    use std::os::windows::ffi::OsStrExt as _;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows::core::PCWSTR;

    let path = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut available = 0_u64;
    let mut total = 0_u64;
    unsafe {
        GetDiskFreeSpaceExW(
            PCWSTR(path.as_ptr()),
            Some(&mut available),
            Some(&mut total),
            None,
        )
    }
    .map_err(|_| "Creation storage capacity is unavailable.".to_string())?;
    Ok((available, total))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn unavailable_destination_volume_domains_normalize_without_probes() {
        for (left, right) in [
            (r"C:\missing\output", r"\\?\C:\other\output"),
            (r"\\server\sháre\missing", r"\\?\UNC\SERVER\SHÁRE\other"),
        ] {
            let left = volume_key(Path::new(left)).unwrap();
            let right = volume_key(Path::new(right)).unwrap();
            assert!(crate::overlay::creation_path::same(&left, &right).unwrap());
        }
    }

    #[test]
    fn unavailable_destination_volume_lookup_counts_all_ordinal_aliases() {
        let mut reservations = HashMap::from([
            (r"\\server\sháre".to_string(), 1024),
            (r"\\SERVER\SHÁRE".to_string(), 2048),
            (r"C:".to_string(), 17),
        ]);
        assert_eq!(
            volume_bytes(&reservations, r"\\Server\Sháre").unwrap(),
            3072
        );
        assert_eq!(volume_bytes(&reservations, r"c:").unwrap(), 17);
        reservations.insert(r"\\server\sháre".to_string(), u64::MAX);
        assert!(volume_bytes(&reservations, r"\\Server\Sháre").is_err());
    }
}
