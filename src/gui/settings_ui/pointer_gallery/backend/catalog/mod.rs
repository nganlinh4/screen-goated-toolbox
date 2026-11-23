use std::path::{Path, PathBuf};

mod collections;
mod github_pins;
mod maps;

/// Packs downloaded before sources were digest-pinned stay in the legacy
/// per-id folders; only bytes that passed verification ever enter this store.
const VERIFIED_PACKS_DIR: &str = "verified-packs";

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

/// Exact size and SHA-256 of one upstream download.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PinnedBytes {
    pub(super) size: u64,
    pub(super) sha256: &'static str,
}

/// Raw files of one immutable commit; `dirs` are already URL-encoded.
/// Each `files` entry is `(dir index, size, SHA-256)` in `REQUIRED_FILES` order.
pub(super) struct GithubSource {
    pub(super) raw_root: &'static str,
    pub(super) dirs: &'static [&'static str],
    pub(super) files: [(usize, u64, &'static str); REQUIRED_FILES.len()],
}

impl GithubSource {
    pub(super) fn file_pin(&self, index: usize) -> PinnedBytes {
        let (_, size, sha256) = self.files[index];
        PinnedBytes { size, sha256 }
    }

    pub(super) fn file_url(&self, index: usize, source_name: &str) -> String {
        format!(
            "{}/{}/{}",
            self.raw_root,
            self.dirs[self.files[index].0],
            urlencoding::encode(source_name)
        )
    }
}

#[derive(Clone, Copy)]
pub(super) enum CollectionSource {
    Github(&'static GithubSource),
    ZipArchive {
        url: &'static str,
        subdir: &'static str,
        pin: PinnedBytes,
    },
    RarArchive {
        url: &'static str,
        subdir: &'static str,
        pin: PinnedBytes,
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
        cache_root.join(VERIFIED_PACKS_DIR).join(self.id)
    }

    /// Unverified folder from before digest pinning; never read, only removed.
    pub(crate) fn legacy_dir(self, cache_root: &Path) -> PathBuf {
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

#[cfg(test)]
mod pin_tests;
