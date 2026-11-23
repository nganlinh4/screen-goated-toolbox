use std::cmp::Ordering;
use std::path::Path;

use windows::Win32::Globalization::{
    CSTR_EQUAL, CSTR_GREATER_THAN, CSTR_LESS_THAN, CompareStringOrdinal,
};

pub(super) struct WindowsPath(Vec<u16>);

impl WindowsPath {
    pub(super) fn new(path: &Path) -> Self {
        use std::os::windows::ffi::OsStrExt as _;
        Self(
            path.as_os_str()
                .encode_wide()
                .map(|unit| {
                    if unit == u16::from(b'/') {
                        u16::from(b'\\')
                    } else {
                        unit
                    }
                })
                .collect(),
        )
    }
}

impl Ord for WindowsPath {
    fn cmp(&self, other: &Self) -> Ordering {
        // Callers first validate the bounded Unicode payload path contract.
        match unsafe { CompareStringOrdinal(&self.0, &other.0, true) } {
            CSTR_LESS_THAN => Ordering::Less,
            CSTR_EQUAL => Ordering::Equal,
            CSTR_GREATER_THAN => Ordering::Greater,
            _ => panic!("Windows ordinal path comparison failed"),
        }
    }
}
impl PartialOrd for WindowsPath {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl PartialEq for WindowsPath {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for WindowsPath {}
