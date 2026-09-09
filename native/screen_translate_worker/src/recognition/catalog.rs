use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::{
    collections::HashSet,
    os::windows::fs::MetadataExt,
    path::{Component, Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Catalog {
    pub schema: u32,
    pub router: String,
    pub labels: String,
    pub readers: Vec<ReaderSpec>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReaderSpec {
    pub id: String,
    pub model: String,
    pub config: Option<String>,
    #[serde(default)]
    pub reverse: bool,
    pub scripts: Vec<String>,
}

impl Catalog {
    pub fn read(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        if bytes.len() > 64 * 1024 {
            bail!("reader catalog exceeds size limit");
        }
        let catalog: Self = serde_json::from_slice(&bytes)?;
        if catalog.schema != 1 || catalog.readers.is_empty() || catalog.readers.len() > 32 {
            bail!("invalid reader catalog");
        }
        let mut ids = HashSet::new();
        let mut scripts = HashSet::new();
        for reader in &catalog.readers {
            if reader.id.is_empty() || !ids.insert(&reader.id) {
                bail!("duplicate reader identity");
            }
            for script in &reader.scripts {
                if script.is_empty() || !scripts.insert(script) {
                    bail!("ambiguous reader coverage");
                }
            }
        }
        Ok(catalog)
    }
}

pub(super) fn file(root: &Path, relative: &str) -> Result<PathBuf> {
    let relative = Path::new(relative);
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        bail!("model path must be package-relative");
    }
    let mut path = root.to_path_buf();
    for part in relative.components() {
        path.push(part);
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.file_attributes() & 0x400 != 0 {
            bail!("model path contains a reparse point");
        }
    }
    if !path.is_file() {
        bail!("model path is not a file");
    }
    path.canonicalize().context("resolve packaged model")
}
