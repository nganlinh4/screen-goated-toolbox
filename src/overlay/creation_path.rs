//! Ordinal path comparison for Creation's Windows file ownership checks.

/// A saved location key, never an ownership grant or an availability probe.
pub(crate) fn normalized_absolute(path: &std::path::Path) -> Result<std::path::PathBuf, String> {
    use std::path::Component;
    if !path.is_absolute() || path.to_string_lossy().contains('\0') {
        return Err("Creation saved path is invalid.".to_string());
    }
    #[cfg(windows)]
    let mut key = {
        use std::ffi::OsString;
        use std::path::Prefix;
        let Some(Component::Prefix(prefix)) = path.components().next() else {
            return Err("Creation saved path is invalid.".to_string());
        };
        let mut value = OsString::new();
        match prefix.kind() {
            Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => {
                value.push(format!("{}:", char::from(drive)))
            }
            Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => {
                value.push(r"\\");
                value.push(server);
                value.push(r"\");
                value.push(share);
            }
            Prefix::Verbatim(name) => {
                value.push(r"\\?\");
                value.push(name);
            }
            Prefix::DeviceNS(name) => {
                value.push(r"\\.\");
                value.push(name);
            }
        }
        std::path::PathBuf::from(value)
    };
    #[cfg(not(windows))]
    let mut key = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) => {}
            Component::RootDir => key.push(std::path::MAIN_SEPARATOR_STR),
            Component::CurDir => {}
            Component::Normal(value) => {
                let mut part = std::path::Path::new(value).components();
                if !matches!(part.next(), Some(Component::Normal(_))) || part.next().is_some() {
                    return Err("Creation saved path is invalid.".to_string());
                }
                key.push(value);
            }
            Component::ParentDir => return Err("Creation saved path is invalid.".to_string()),
        }
    }
    Ok(key)
}

#[cfg(windows)]
pub(crate) fn same(left: &str, right: &str) -> Result<bool, String> {
    use windows::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};
    let left = left.encode_utf16().collect::<Vec<_>>();
    let right = right.encode_utf16().collect::<Vec<_>>();
    let result = unsafe { CompareStringOrdinal(&left, &right, true) };
    if result.0 == 0 {
        Err("Creation path comparison is unavailable.".to_string())
    } else {
        Ok(result == CSTR_EQUAL)
    }
}

#[cfg(not(windows))]
pub(crate) fn same(left: &str, right: &str) -> Result<bool, String> {
    Ok(left == right)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn unavailable_destination_keys_normalize_prefixes_without_io() {
        for (left, right) in [
            (r"C:\missing\RÉSUMÉ.png", r"\\?\C:\missing\résumé.PNG"),
            (
                r"\\server\share\missing\result.png",
                r"\\?\UNC\SERVER\SHARE\missing\result.png",
            ),
        ] {
            let left = normalized_absolute(Path::new(left)).unwrap();
            let right = normalized_absolute(Path::new(right)).unwrap();
            assert!(same(&left.to_string_lossy(), &right.to_string_lossy()).unwrap());
        }
        for invalid in [
            r"C:relative\result.png",
            r"C:\root\..\result.png",
            r"\\?\C:\root\..\result.png",
        ] {
            assert!(
                normalized_absolute(Path::new(invalid)).is_err(),
                "{invalid}"
            );
        }
        assert!(!same(r"C:\résumé.png", r"C:\other.png").unwrap());
    }
}
