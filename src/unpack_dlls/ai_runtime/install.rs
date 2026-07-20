use anyhow::{Context, Result, anyhow};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::AiRuntimeUi;
use super::packages::{
    PACKAGES, RuntimePackage, expected_runtime_marker_contents, package_entries, package_name,
    runtime_arch, runtime_marker_path,
};
use super::progress::update_progress;

fn download_package(
    package: RuntimePackage,
    bin_dir: &Path,
    stop_signal: &AtomicBool,
    ui: AiRuntimeUi,
) -> Result<PathBuf> {
    let archive_path = bin_dir.join(package.archive_name);
    let temp_path = archive_path.with_extension("tmp");
    let _ = fs::remove_file(&temp_path);

    let response = ureq::get(package.url)
        .header("User-Agent", "ScreenGoatedToolbox")
        .call()
        .map_err(|e| anyhow!("Download failed for {}: {}", package.label, e))?;

    let total_size = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);

    let mut reader = response.into_body().into_reader();
    let mut file = fs::File::create(&temp_path)
        .with_context(|| format!("Failed to create temp file '{}'", temp_path.display()))?;
    let mut downloaded = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    let mut last_update = Instant::now() - Duration::from_secs(1);

    loop {
        if stop_signal.load(Ordering::Relaxed) {
            let _ = fs::remove_file(&temp_path);
            return Err(anyhow!("Download cancelled"));
        }

        let bytes_read = match reader.read(&mut buffer) {
            Ok(bytes_read) => bytes_read,
            Err(err) => {
                let _ = fs::remove_file(&temp_path);
                return Err(err.into());
            }
        };
        if bytes_read == 0 {
            break;
        }

        if let Err(err) = file.write_all(&buffer[..bytes_read]) {
            let _ = fs::remove_file(&temp_path);
            return Err(err.into());
        }
        downloaded += bytes_read as u64;

        if total_size > 0 && last_update.elapsed() >= Duration::from_millis(100) {
            let local_progress = downloaded as f32 / total_size as f32;
            let progress = package.progress_start
                + (package.progress_end - package.progress_start) * local_progress;
            let detail = format!(
                "{} ({:.1} MB / {:.1} MB)",
                package.label,
                downloaded as f64 / 1024.0 / 1024.0,
                total_size as f64 / 1024.0 / 1024.0
            );
            update_progress(ui, &detail, progress);
            last_update = Instant::now();
        }
    }

    drop(file);
    if let Err(err) = fs::rename(&temp_path, &archive_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(anyhow!(
            "Failed to move downloaded runtime archive into '{}': {}",
            archive_path.display(),
            err
        ));
    }

    Ok(archive_path)
}

fn install_package_with_ranged_fetch(
    package: RuntimePackage,
    bin_dir: &Path,
    stop_signal: &AtomicBool,
    ui: AiRuntimeUi,
) -> Result<()> {
    let arch = runtime_arch();
    let entries = package_entries(package);
    let badge = crate::overlay::auto_copy_badge::locale_text();
    let name = package_name(package);
    let arch_text = arch.to_string();
    update_progress(
        ui,
        &crate::overlay::auto_copy_badge::format_locale(
            badge.preparing_payload_fmt,
            &[("name", name), ("arch", &arch_text)],
        ),
        package.progress_start,
    );

    let label = name;
    super::super::remote_zip::download_entries_to_dir(
        package.url,
        entries,
        bin_dir,
        stop_signal,
        |downloaded, total| {
            let fraction = downloaded as f32 / total.max(1) as f32;
            let progress =
                package.progress_start + (package.progress_end - package.progress_start) * fraction;
            let downloaded = format!("{:.1}", downloaded as f64 / 1024.0 / 1024.0);
            let total = format!("{:.1}", total as f64 / 1024.0 / 1024.0);
            let detail = crate::overlay::auto_copy_badge::format_locale(
                badge.downloading_payload_fmt,
                &[
                    ("name", label),
                    ("arch", &arch_text),
                    ("downloaded", &downloaded),
                    ("total", &total),
                ],
            );
            update_progress(ui, &detail, progress);
        },
    )?;

    update_progress(
        ui,
        &crate::overlay::auto_copy_badge::format_locale(
            badge.installing_payload_fmt,
            &[("name", label), ("arch", &arch_text)],
        ),
        package.progress_end,
    );
    Ok(())
}

fn extract_package(
    package: RuntimePackage,
    archive_path: &Path,
    bin_dir: &Path,
    stop_signal: &AtomicBool,
    ui: AiRuntimeUi,
) -> Result<()> {
    let entries = package_entries(package);
    let file = fs::File::open(archive_path)
        .with_context(|| format!("Failed to open archive '{}'", archive_path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("Failed to read archive '{}'", archive_path.display()))?;

    let extraction_progress = package.progress_end + 2.0;
    let badge = crate::overlay::auto_copy_badge::locale_text();
    let extract_label = crate::overlay::auto_copy_badge::format_locale(
        badge.extracting_package_fmt,
        &[("name", package_name(package))],
    );
    update_progress(ui, &extract_label, extraction_progress);

    for entry in entries {
        if stop_signal.load(Ordering::Relaxed) {
            return Err(anyhow!("Download cancelled"));
        }

        let mut zipped = archive.by_name(entry.source_path).with_context(|| {
            format!(
                "Missing '{}' in runtime archive '{}'",
                entry.source_path,
                archive_path.display()
            )
        })?;

        let final_path = bin_dir.join(entry.dest_name);
        let temp_path = final_path.with_extension("tmp");
        let _ = fs::remove_file(&temp_path);

        let mut out = fs::File::create(&temp_path)
            .with_context(|| format!("Failed to create '{}'", temp_path.display()))?;
        if let Err(err) = std::io::copy(&mut zipped, &mut out) {
            let _ = fs::remove_file(&temp_path);
            return Err(anyhow!("Failed to extract '{}': {}", entry.dest_name, err));
        }
        drop(out);

        if final_path.exists() {
            let _ = fs::remove_file(&final_path);
        }
        if let Err(err) = fs::rename(&temp_path, &final_path) {
            let _ = fs::remove_file(&temp_path);
            return Err(anyhow!(
                "Failed to finalize '{}': {}",
                final_path.display(),
                err
            ));
        }
    }

    let _ = fs::remove_file(archive_path);
    Ok(())
}

pub(super) fn install_runtime(stop_signal: &AtomicBool, ui: AiRuntimeUi) -> Result<()> {
    let bin_dir = super::super::private_bin_dir();
    fs::create_dir_all(&bin_dir)
        .with_context(|| format!("Failed to create '{}'", bin_dir.display()))?;

    for package in PACKAGES {
        if let Err(err) = install_package_with_ranged_fetch(*package, &bin_dir, stop_signal, ui) {
            crate::log_info!(
                "[AI Runtime] Ranged fetch for {} failed, falling back to full package download: {}",
                package_name(*package),
                err
            );
            let archive_path = download_package(*package, &bin_dir, stop_signal, ui)?;
            extract_package(*package, &archive_path, &bin_dir, stop_signal, ui)?;
        }
    }

    let badge = crate::overlay::auto_copy_badge::locale_text();
    update_progress(ui, badge.finalizing_local_ai_runtime, 100.0);
    fs::write(
        runtime_marker_path(&bin_dir),
        expected_runtime_marker_contents(),
    )
    .with_context(|| {
        format!(
            "Failed to write '{}'",
            runtime_marker_path(&bin_dir).display()
        )
    })?;
    Ok(())
}
