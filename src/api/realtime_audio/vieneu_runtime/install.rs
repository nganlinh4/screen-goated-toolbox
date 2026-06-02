use super::{MIN_RUNTIME_ABI, RUNTIME_MANIFEST_URL, VieneuRuntimeChunk, VieneuRuntimeManifest};
use anyhow::{Context, Result, anyhow, bail};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

pub(super) fn install_managed_runtime(stage: &Path, dir: &Path, entrypoint: &str) -> Result<()> {
    let managed_root_name = entrypoint
        .split(['/', '\\'])
        .next()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("vieneu-sidecar");
    let stage_managed_root = stage.join(managed_root_name);
    let dest_managed_root = dir.join(managed_root_name);
    if !stage_managed_root.is_dir() {
        bail!(
            "VieNeu runtime archive is missing managed root '{}'",
            stage_managed_root.display()
        );
    }
    if dest_managed_root.exists() {
        fs::remove_dir_all(&dest_managed_root).with_context(|| {
            format!(
                "Failed to remove previous managed VieNeu runtime '{}'",
                dest_managed_root.display()
            )
        })?;
    }
    if fs::rename(&stage_managed_root, &dest_managed_root).is_err() {
        copy_dir_all(&stage_managed_root, &dest_managed_root).with_context(|| {
            format!(
                "Failed to copy VieNeu runtime from '{}' to '{}'",
                stage_managed_root.display(),
                dest_managed_root.display()
            )
        })?;
        fs::remove_dir_all(&stage_managed_root).with_context(|| {
            format!(
                "Failed to remove copied VieNeu staging root '{}'",
                stage_managed_root.display()
            )
        })?;
    }
    Ok(())
}

pub(super) fn fetch_manifest() -> Result<VieneuRuntimeManifest> {
    let response = ureq::get(RUNTIME_MANIFEST_URL)
        .header("User-Agent", "ScreenGoatedToolbox")
        .call()
        .map_err(|err| anyhow!("Failed to fetch VieNeu runtime manifest: {err}"))?;
    let mut body = String::new();
    response
        .into_body()
        .into_reader()
        .read_to_string(&mut body)?;
    serde_json::from_str(&body)
        .map_err(|err| anyhow!("Failed to parse VieNeu runtime manifest: {err}"))
}

pub(super) fn validate_manifest(manifest: &VieneuRuntimeManifest) -> Result<()> {
    if manifest.abi_version < MIN_RUNTIME_ABI {
        bail!(
            "VieNeu runtime ABI {} is older than required ABI {}",
            manifest.abi_version,
            MIN_RUNTIME_ABI
        );
    }
    if manifest.entrypoint.trim().is_empty() || manifest.entrypoint.contains("..") {
        bail!("VieNeu runtime manifest has an unsafe entrypoint");
    }
    if manifest.chunks.is_empty() {
        bail!("VieNeu runtime manifest has no downloadable chunks");
    }
    for chunk in &manifest.chunks {
        if chunk.filename.trim().is_empty()
            || chunk.filename.contains("..")
            || chunk.sha256.trim().len() != 64
            || chunk.size == 0
        {
            bail!("VieNeu runtime manifest has an invalid chunk entry");
        }
    }
    Ok(())
}

pub(super) fn download_verified_chunk(
    chunk: &VieneuRuntimeChunk,
    path: &Path,
    stop_signal: &AtomicBool,
    on_progress: impl Fn(u64),
) -> Result<()> {
    crate::api::realtime_audio::model_loader::download_file_with_progress(
        &chunk.url,
        path,
        stop_signal,
        |downloaded, _total| on_progress(downloaded),
    )
    .with_context(|| {
        format!(
            "Failed to download VieNeu runtime chunk '{}' to '{}'",
            chunk.filename,
            path.display()
        )
    })?;
    let metadata = fs::metadata(path)
        .with_context(|| format!("Failed to stat VieNeu runtime chunk '{}'", path.display()))?;
    if metadata.len() != chunk.size {
        let _ = fs::remove_file(path);
        bail!(
            "VieNeu runtime chunk '{}' size mismatch: expected {}, got {}",
            chunk.filename,
            chunk.size,
            metadata.len()
        );
    }
    let actual = sha256_hex(path)?;
    if actual != chunk.sha256.to_ascii_lowercase() {
        let _ = fs::remove_file(path);
        bail!(
            "VieNeu runtime chunk '{}' checksum mismatch",
            chunk.filename
        );
    }
    Ok(())
}

pub(super) fn concatenate_chunks(chunks: &[PathBuf], output: &Path) -> Result<()> {
    let mut out = fs::File::create(output)
        .with_context(|| format!("Failed to create VieNeu archive '{}'", output.display()))?;
    for chunk in chunks {
        let mut input = fs::File::open(chunk)
            .with_context(|| format!("Failed to open VieNeu chunk '{}'", chunk.display()))?;
        std::io::copy(&mut input, &mut out).with_context(|| {
            format!(
                "Failed to append VieNeu chunk '{}' into '{}'",
                chunk.display(),
                output.display()
            )
        })?;
    }
    out.flush()
        .with_context(|| format!("Failed to flush VieNeu archive '{}'", output.display()))?;
    Ok(())
}

pub(super) fn extract_runtime_archive(archive: &Path, stage: &Path) -> Result<()> {
    fs::create_dir_all(stage).with_context(|| {
        format!(
            "Failed to create VieNeu extraction dir '{}'",
            stage.display()
        )
    })?;
    let tar = if cfg!(windows) { "tar.exe" } else { "tar" };
    let output = Command::new(tar)
        .arg("-xf")
        .arg(archive)
        .arg("-C")
        .arg(stage)
        .output()
        .with_context(|| format!("Failed to start {tar} for '{}'", archive.display()))?;
    if !output.status.success() {
        bail!(
            "{tar} failed to extract VieNeu runtime archive '{}' into '{}' (status: {}). stdout: {} stderr: {}",
            archive.display(),
            stage.display(),
            output.status,
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let _ = fs::remove_file(archive);
    Ok(())
}

fn copy_dir_all(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if entry.metadata()?.is_dir() {
            copy_dir_all(&src, &dst)?;
        } else {
            fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

fn sha256_hex(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
