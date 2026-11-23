//! Conversion between Windows verbatim (`\\?\`) and ordinary path spellings.

use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt as _, OsStringExt as _};
use std::path::{Path, PathBuf};

/// Longest path, in UTF-16 units, that legacy Win32 consumers accept.
const LEGACY_MAX_PATH_UNITS: usize = 259;

/// Return the ordinary-namespace spelling of `path`, or `None` when removing
/// the verbatim prefix would let Win32 normalization change which file it
/// names (empty, `.`/`..`, trailing dot or space components, or `/`).
/// Non-verbatim paths are returned unchanged.
pub(crate) fn ordinary_namespace(path: &Path, units: Vec<u16>) -> Option<Vec<u16>> {
    use std::path::{Component, Prefix};
    let Some(Component::Prefix(prefix)) = path.components().next() else {
        return None;
    };
    let (skip, unc) = match prefix.kind() {
        Prefix::VerbatimDisk(_) => (4, false),
        Prefix::VerbatimUNC(_, _) => (8, true),
        _ => return Some(units),
    };
    let tail = units.get(skip..)?;
    let checked = tail.strip_suffix(&[92]).unwrap_or(tail);
    if checked.contains(&47)
        || checked.split(|unit| *unit == 92).any(|part| {
            part.is_empty()
                || part == [46]
                || part == [46, 46]
                || matches!(part.last(), Some(46 | 32))
        })
    {
        return None;
    }
    Some(if unc {
        [vec![92, 92], tail.to_vec()].concat()
    } else {
        tail.to_vec()
    })
}

/// Spell a path the way users and follow-up commands expect (`C:\dir`,
/// `\\server\share`), keeping the verbatim form whenever the ordinary one
/// would be ambiguous or too long for legacy tools.
pub(crate) fn display_path(path: &Path) -> PathBuf {
    let units: Vec<u16> = path.as_os_str().encode_wide().collect();
    match ordinary_namespace(path, units) {
        Some(ordinary) if ordinary.len() <= LEGACY_MAX_PATH_UNITS => {
            PathBuf::from(OsString::from_wide(&ordinary))
        }
        _ => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_path_strips_only_unambiguous_verbatim_prefixes() {
        for (verbatim, ordinary) in [
            (r"\\?\C:\Users\me\Downloads", r"C:\Users\me\Downloads"),
            (r"\\?\C:\", r"C:\"),
            (r"\\?\UNC\server\share\dir", r"\\server\share\dir"),
            (r"C:\already\ordinary", r"C:\already\ordinary"),
        ] {
            assert_eq!(display_path(Path::new(verbatim)), Path::new(ordinary));
        }
        for kept in [
            r"\\?\C:\dir.\file",
            r"\\?\C:\dir \file",
            r"\\?\C:\dir\..\file",
            r"\\?\Volume{00000000-0000-0000-0000-000000000000}\dir",
        ] {
            assert_eq!(display_path(Path::new(kept)), Path::new(kept));
        }
        let long = format!(r"\\?\C:\{}", "a".repeat(LEGACY_MAX_PATH_UNITS));
        assert_eq!(display_path(Path::new(&long)), Path::new(&long));
    }
}
