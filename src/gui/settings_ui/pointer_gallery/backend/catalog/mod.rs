use std::path::{Path, PathBuf};

mod collections;
mod maps;

pub(super) const REQUIRED_FILES: [&str; 17] = [
    "pointer.cur",
    "help.cur",
    "working.ani",
    "busy.ani",
    "precision.cur",
    "beam.cur",
    "handwriting.cur",
    "unavailable.cur",
    "vert.cur",
    "horz.cur",
    "dgn1.cur",
    "dgn2.cur",
    "move.cur",
    "alternate.cur",
    "link.cur",
    "person.cur",
    "pin.cur",
];

pub(super) const SCHEME_FILE_ORDER: [&str; 17] = REQUIRED_FILES;

pub(super) const REGISTRY_FILE_MAP: [(&str, &str); 19] = [
    ("AppStarting", "working.ani"),
    ("Arrow", "pointer.cur"),
    ("Crosshair", "precision.cur"),
    ("precisionhair", "precision.cur"),
    ("Hand", "link.cur"),
    ("Help", "help.cur"),
    ("IBeam", "beam.cur"),
    ("No", "unavailable.cur"),
    ("NWPen", "handwriting.cur"),
    ("SizeAll", "move.cur"),
    ("SizeNESW", "dgn2.cur"),
    ("SizeNS", "vert.cur"),
    ("SizeNWSE", "dgn1.cur"),
    ("SizeWE", "horz.cur"),
    ("UpArrow", "alternate.cur"),
    ("Wait", "busy.ani"),
    ("Person", "person.cur"),
    ("Pin", "pin.cur"),
    ("OCR Normal Select", "pointer.cur"),
];

#[derive(Clone, Copy)]
pub(super) enum CollectionSource {
    GithubApi(&'static [&'static str]),
    ZipArchive {
        url: &'static str,
        subdir: &'static str,
    },
    RarArchive {
        url: &'static str,
        subdir: &'static str,
    },
}

#[derive(Clone, Copy)]
pub(crate) struct CursorCollectionSpec {
    pub(crate) id: &'static str,
    pub(crate) title: &'static str,
    pub(crate) scheme_name: &'static str,
    pub(super) source: CollectionSource,
    pub(super) file_name_map: &'static [(&'static str, &'static str)],
}

impl CursorCollectionSpec {
    pub(crate) fn local_dir(self, cache_root: &Path) -> PathBuf {
        cache_root.join(self.id)
    }
}

pub(super) fn source_name_for_file(spec: CursorCollectionSpec, local_name: &str) -> &str {
    spec.file_name_map
        .iter()
        .find_map(|(local, source)| (*local == local_name).then_some(*source))
        .unwrap_or(local_name)
}

pub(super) fn collections() -> &'static [CursorCollectionSpec] {
    &collections::COLLECTIONS
}
