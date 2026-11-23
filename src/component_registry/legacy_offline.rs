//! One-click removal of offline speech engines that earlier releases installed.
//!
//! Those engines are gone (see `docs/OFFLINE-ENGINES.md`), but their models,
//! runtimes and working files can remain on disk. This module lists only their
//! exact known folders under the app data roots, so Downloaded Tools can show
//! their size and remove them when the user asks.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

/// Registry ids of removed engines: their `components/<id>` folders and their
/// `<id>-...` entries in the registry's working directories.
const COMPONENT_IDS: [&str; 13] = [
    "kokoro-82m-v1-model",
    "step-audio-editx-model",
    "magpie-multilingual-357m-model",
    "vieneu-v2-turbo-model",
    "step-audio-runtime",
    "magpie-runtime",
    "vieneu-runtime",
    "qwen3-asr-0-6b-model",
    "qwen3-asr-1-7b-model",
    "qwen3-cuda-runtime",
    "local-asr-worker",
    "supertonic-3-model",
    "sherpa-onnx-runtime",
];

const WORKING_DIRECTORIES: [&str; 3] = [
    "component-staging",
    "component-downloads",
    "component-partials",
];

/// Folders under the roaming app data root.
const ROAMING_FOLDERS: [&str; 18] = [
    "models/kokoro_v1",
    "models/supertonic_3",
    "models/voxtral_tts_2603",
    "models/magpie_multilingual_357m",
    "models/step_audio_editx",
    "models/parakeet",
    "models/parakeet_tdt_0_6b_v3",
    "models/qwen3_asr_0_6b",
    "models/qwen3_asr_1_7b",
    "models/streaming-zipformer-en-kroko",
    "models/streaming-zipformer-korean",
    "models/streaming-zipformer-zh",
    "models/streaming-zipformer-fr-kroko",
    "models/streaming-zipformer-de-kroko",
    "models/streaming-zipformer-es-kroko",
    "models/streaming-zipformer-small-ru-vosk",
    "models/streaming-zipformer-multilingual-8lang",
    "bin/qwen3_asr_reference",
];

/// Folders under the local app data root.
const LOCAL_FOLDERS: [&str; 8] = [
    "models/magpie_multilingual_357m",
    "models/step_audio_editx",
    "bin/x64/sherpa-onnx",
    "bin/step_audio_runtime",
    "bin/magpie_runtime",
    "bin/vieneu_runtime",
    "tmp/magpie_tts",
    "worker-workspaces/local-asr-worker",
];

const MAX_WORKING_ENTRIES: usize = 4096;
const MAX_SIZE_ENTRIES: usize = 500_000;

/// Legacy folders and files present on disk, with their combined size.
pub(crate) struct LegacyFolders {
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) total_bytes: u64,
}

struct Roots {
    roaming: PathBuf,
    local: PathBuf,
}

impl Roots {
    fn current() -> Self {
        Self {
            roaming: crate::paths::app_runtime_config_dir(),
            local: crate::paths::app_runtime_local_data_dir(),
        }
    }
}

pub(crate) fn scan() -> Result<LegacyFolders> {
    scan_roots(&Roots::current())
}

/// Removes every present legacy folder. Anything reached through a reparse
/// point, or outside the known list, is left untouched.
pub(crate) fn remove() -> Result<()> {
    let _guard = super::acquire_mutation_guard()?;
    remove_roots(&Roots::current())
}

fn scan_roots(roots: &Roots) -> Result<LegacyFolders> {
    let paths = present_paths(roots)?;
    let total_bytes = paths.iter().map(|path| entry_size(path)).sum();
    Ok(LegacyFolders { paths, total_bytes })
}

fn remove_roots(roots: &Roots) -> Result<()> {
    let mut failures = 0;
    for path in present_paths(roots)? {
        if let Err(error) = remove_entry(&path) {
            failures += 1;
            crate::log_info!(
                "[Components] Kept legacy offline entry {}: {error:#}",
                path.display()
            );
        }
    }
    if failures > 0 {
        bail!("{failures} legacy offline folder(s) could not be removed");
    }
    Ok(())
}

fn present_paths(roots: &Roots) -> Result<Vec<PathBuf>> {
    let mut candidates: Vec<(&Path, PathBuf)> = Vec::new();
    candidates.extend(
        ROAMING_FOLDERS
            .iter()
            .map(|path| (roots.roaming.as_path(), PathBuf::from(path))),
    );
    candidates.extend(
        LOCAL_FOLDERS
            .iter()
            .map(|path| (roots.local.as_path(), PathBuf::from(path))),
    );
    candidates.extend(
        COMPONENT_IDS
            .iter()
            .map(|id| (roots.local.as_path(), Path::new("components").join(id))),
    );
    for directory in WORKING_DIRECTORIES {
        for name in owned_working_entries(&roots.local, directory)? {
            candidates.push((roots.local.as_path(), Path::new(directory).join(name)));
        }
    }
    let mut present = Vec::new();
    for (root, relative) in candidates {
        if !regular_parents(root, &relative)? {
            continue;
        }
        let target = root.join(&relative);
        match std::fs::symlink_metadata(&target) {
            Ok(metadata) if !super::is_reparse_point(&metadata) => present.push(target),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(present)
}

/// Names in one registry working directory that belong to a removed engine.
fn owned_working_entries(root: &Path, directory: &str) -> Result<Vec<String>> {
    if !regular_parents(root, &Path::new(directory).join("entry"))? {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in std::fs::read_dir(root.join(directory))?.take(MAX_WORKING_ENTRIES) {
        let Ok(name) = entry?.file_name().into_string() else {
            continue;
        };
        if COMPONENT_IDS
            .iter()
            .any(|id| name.starts_with(&format!("{id}-")))
        {
            names.push(name);
        }
    }
    Ok(names)
}

/// True when `root` and every directory between it and `relative` exist as
/// regular directories, never reparse points.
fn regular_parents(root: &Path, relative: &Path) -> Result<bool> {
    let mut current = root.to_path_buf();
    let mut directories = vec![current.clone()];
    if let Some(parent) = relative.parent() {
        for component in parent.components() {
            current.push(component);
            directories.push(current.clone());
        }
    }
    for directory in directories {
        match std::fs::symlink_metadata(&directory) {
            Ok(metadata) if metadata.is_dir() && !super::is_reparse_point(&metadata) => {}
            Ok(_) => return Ok(false),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(true)
}

fn remove_entry(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if super::is_reparse_point(&metadata) {
        bail!("legacy entry is a reparse point");
    }
    if metadata.is_dir() {
        super::interrupted_installs::remove_tree(path)
    } else if metadata.is_file() {
        Ok(std::fs::remove_file(path)?)
    } else {
        bail!("legacy entry is not a regular file or directory")
    }
}

/// Bytes under `path`, without following reparse points and stopping after a
/// bounded number of entries.
fn entry_size(path: &Path) -> u64 {
    let mut total = 0;
    let mut visited = 0;
    let mut pending = vec![path.to_path_buf()];
    while let Some(current) = pending.pop() {
        let Ok(metadata) = std::fs::symlink_metadata(&current) else {
            continue;
        };
        if super::is_reparse_point(&metadata) {
            continue;
        }
        if metadata.is_file() {
            total += metadata.len();
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            visited += 1;
            if visited > MAX_SIZE_ENTRIES {
                return total;
            }
            pending.push(entry.path());
        }
    }
    total
}

#[cfg(test)]
mod tests;
