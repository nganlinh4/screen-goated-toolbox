use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

pub(super) struct NativeSidecarSupport {
    vc: crate::component_registry::vc_runtime::VcRuntimeUse,
}

impl NativeSidecarSupport {
    pub(super) fn ensure() -> Result<Self> {
        Ok(Self {
            vc: crate::component_registry::vc_runtime::ensure_component(|_, _| {})?,
        })
    }

    pub(super) fn configure(&self, command: &mut Command) -> Result<()> {
        let vc_dir = canonical_regular_dir(self.vc.bin_dir())?;
        let mut paths = vec![vc_dir];
        if let Some(existing) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&existing));
        }
        command.env(
            "PATH",
            std::env::join_paths(paths).context("build native sidecar search path")?,
        );
        Ok(())
    }
}

fn canonical_regular_dir(path: &Path) -> Result<std::path::PathBuf> {
    let canonical = std::fs::canonicalize(path)
        .with_context(|| format!("canonicalize VC runtime directory '{}'", path.display()))?;
    let metadata = std::fs::symlink_metadata(&canonical)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("VC runtime directory is unsafe");
    }
    Ok(canonical)
}

fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(test)]
mod tests {
    #[test]
    fn prepended_search_path_preserves_existing_entries() {
        let vc = std::path::PathBuf::from(r"C:\components\vc\bin\x64");
        let existing = std::env::join_paths([
            std::path::Path::new(r"C:\Windows\System32"),
            std::path::Path::new(r"C:\CUDA\bin"),
        ])
        .unwrap();
        let mut paths = vec![vc.clone()];
        paths.extend(std::env::split_paths(&existing));
        let joined = std::env::join_paths(paths).unwrap();
        assert_eq!(std::env::split_paths(&joined).next(), Some(vc));
    }
}
