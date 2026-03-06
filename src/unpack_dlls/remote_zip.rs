use anyhow::{Context, Result, anyhow};
use flate2::read::DeflateDecoder;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

const EOCD_SIGNATURE: u32 = 0x0605_4B50;
const CENTRAL_DIRECTORY_SIGNATURE: u32 = 0x0201_4B50;
const LOCAL_FILE_HEADER_SIGNATURE: u32 = 0x0403_4B50;
const TAIL_RANGE_BYTES: u64 = 256 * 1024;

#[derive(Clone, Copy)]
pub(super) struct RequestedZipEntry {
    pub source_path: &'static str,
    pub dest_name: &'static str,
}

#[derive(Clone)]
struct CentralDirectoryEntry {
    compression_method: u16,
    compressed_size: u64,
    uncompressed_size: u64,
    local_header_offset: u64,
}

fn le_u16(bytes: &[u8], offset: usize) -> Result<u16> {
    bytes
        .get(offset..offset + 2)
        .map(|slice| u16::from_le_bytes([slice[0], slice[1]]))
        .ok_or_else(|| anyhow!("Truncated ZIP metadata"))
}

fn le_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    bytes
        .get(offset..offset + 4)
        .map(|slice| u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
        .ok_or_else(|| anyhow!("Truncated ZIP metadata"))
}

fn fetch_range_with_progress<F>(
    url: &str,
    range: &str,
    stop_signal: &AtomicBool,
    mut on_chunk: F,
) -> Result<Vec<u8>>
where
    F: FnMut(usize),
{
    if stop_signal.load(Ordering::Relaxed) {
        return Err(anyhow!("Download cancelled"));
    }

    let response = ureq::get(url)
        .header("User-Agent", "ScreenGoatedToolbox")
        .header("Range", range)
        .call()
        .map_err(|e| anyhow!("Range request failed for {}: {}", range, e))?;

    let status = response.status().as_u16();
    if status != 206 {
        return Err(anyhow!(
            "Expected HTTP 206 for range request {}, got {}",
            range,
            status
        ));
    }

    let mut reader = response.into_body().into_reader();
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        if stop_signal.load(Ordering::Relaxed) {
            return Err(anyhow!("Download cancelled"));
        }

        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }

        bytes.extend_from_slice(&buffer[..bytes_read]);
        on_chunk(bytes_read);
    }
    Ok(bytes)
}

fn fetch_range(url: &str, range: &str, stop_signal: &AtomicBool) -> Result<Vec<u8>> {
    fetch_range_with_progress(url, range, stop_signal, |_| {})
}

fn locate_end_of_central_directory(tail: &[u8]) -> Result<(u64, u64)> {
    if tail.len() < 22 {
        return Err(anyhow!("ZIP tail too small to contain EOCD"));
    }

    for index in (0..=tail.len() - 22).rev() {
        if le_u32(tail, index)? != EOCD_SIGNATURE {
            continue;
        }

        let comment_len = le_u16(tail, index + 20)? as usize;
        if index + 22 + comment_len > tail.len() {
            continue;
        }

        let central_directory_size = le_u32(tail, index + 12)? as u64;
        let central_directory_offset = le_u32(tail, index + 16)? as u64;
        return Ok((central_directory_offset, central_directory_size));
    }

    Err(anyhow!(
        "Could not find ZIP end-of-central-directory record"
    ))
}

fn parse_central_directory(bytes: &[u8]) -> Result<HashMap<String, CentralDirectoryEntry>> {
    let mut offset = 0usize;
    let mut entries = HashMap::new();

    while offset < bytes.len() {
        if offset + 46 > bytes.len() {
            return Err(anyhow!("Truncated ZIP central directory"));
        }
        if le_u32(bytes, offset)? != CENTRAL_DIRECTORY_SIGNATURE {
            return Err(anyhow!("Invalid ZIP central directory signature"));
        }

        let compression_method = le_u16(bytes, offset + 10)?;
        let compressed_size = le_u32(bytes, offset + 20)? as u64;
        let uncompressed_size = le_u32(bytes, offset + 24)? as u64;
        let file_name_len = le_u16(bytes, offset + 28)? as usize;
        let extra_len = le_u16(bytes, offset + 30)? as usize;
        let comment_len = le_u16(bytes, offset + 32)? as usize;
        let local_header_offset = le_u32(bytes, offset + 42)? as u64;

        if compressed_size == u32::MAX as u64
            || uncompressed_size == u32::MAX as u64
            || local_header_offset == u32::MAX as u64
        {
            return Err(anyhow!(
                "ZIP64 packages are not supported in ranged downloader"
            ));
        }

        let name_start = offset + 46;
        let name_end = name_start + file_name_len;
        let file_name = std::str::from_utf8(
            bytes
                .get(name_start..name_end)
                .ok_or_else(|| anyhow!("Invalid ZIP file name bounds"))?,
        )
        .context("ZIP file name was not valid UTF-8")?
        .to_string();

        entries.insert(
            file_name,
            CentralDirectoryEntry {
                compression_method,
                compressed_size,
                uncompressed_size,
                local_header_offset,
            },
        );

        offset = name_end + extra_len + comment_len;
    }

    Ok(entries)
}

fn read_entry_data_offset(
    url: &str,
    local_header_offset: u64,
    stop_signal: &AtomicBool,
) -> Result<u64> {
    let header_probe = fetch_range(
        url,
        &format!(
            "bytes={}-{}",
            local_header_offset,
            local_header_offset + 255
        ),
        stop_signal,
    )?;

    if header_probe.len() < 30 {
        return Err(anyhow!("Truncated ZIP local header"));
    }
    if le_u32(&header_probe, 0)? != LOCAL_FILE_HEADER_SIGNATURE {
        return Err(anyhow!("Invalid ZIP local header signature"));
    }

    let file_name_len = le_u16(&header_probe, 26)? as u64;
    let extra_len = le_u16(&header_probe, 28)? as u64;
    Ok(local_header_offset + 30 + file_name_len + extra_len)
}

fn write_downloaded_entry(
    dest_dir: &Path,
    dest_name: &str,
    compression_method: u16,
    compressed_bytes: &[u8],
    uncompressed_size: u64,
    stop_signal: &AtomicBool,
) -> Result<()> {
    let final_path = dest_dir.join(dest_name);
    let temp_path = final_path.with_extension("tmp");
    let _ = fs::remove_file(&temp_path);

    let mut output = fs::File::create(&temp_path)
        .with_context(|| format!("Failed to create '{}'", temp_path.display()))?;

    match compression_method {
        0 => {
            output.write_all(compressed_bytes)?;
        }
        8 => {
            let mut decoder = DeflateDecoder::new(compressed_bytes);
            std::io::copy(&mut decoder, &mut output)
                .with_context(|| format!("Failed to inflate '{}'", dest_name))?;
        }
        other => {
            let _ = fs::remove_file(&temp_path);
            return Err(anyhow!(
                "Unsupported ZIP compression method {} for '{}'",
                other,
                dest_name
            ));
        }
    }

    drop(output);

    let written_size = fs::metadata(&temp_path)
        .with_context(|| format!("Failed to stat '{}'", temp_path.display()))?
        .len();
    if written_size != uncompressed_size {
        let _ = fs::remove_file(&temp_path);
        return Err(anyhow!(
            "Unexpected size for '{}': wrote {} bytes, expected {}",
            dest_name,
            written_size,
            uncompressed_size
        ));
    }

    if stop_signal.load(Ordering::Relaxed) {
        let _ = fs::remove_file(&temp_path);
        return Err(anyhow!("Download cancelled"));
    }

    if final_path.exists() {
        let _ = fs::remove_file(&final_path);
    }
    fs::rename(&temp_path, &final_path)
        .with_context(|| format!("Failed to finalize '{}'", final_path.display()))?;

    Ok(())
}

pub(super) fn download_entries_to_dir<F>(
    url: &str,
    entries: &[RequestedZipEntry],
    dest_dir: &Path,
    stop_signal: &AtomicBool,
    mut on_progress: F,
) -> Result<()>
where
    F: FnMut(u64, u64),
{
    let tail = fetch_range(url, &format!("bytes=-{}", TAIL_RANGE_BYTES), stop_signal)?;
    let (central_directory_offset, central_directory_size) =
        locate_end_of_central_directory(&tail)?;
    let central_directory = fetch_range(
        url,
        &format!(
            "bytes={}-{}",
            central_directory_offset,
            central_directory_offset + central_directory_size.saturating_sub(1)
        ),
        stop_signal,
    )?;
    let parsed_entries = parse_central_directory(&central_directory)?;

    let total_download_bytes = entries.iter().try_fold(0u64, |acc, requested| {
        let entry = parsed_entries
            .get(requested.source_path)
            .ok_or_else(|| anyhow!("Missing '{}' in remote ZIP", requested.source_path))?;
        Ok::<u64, anyhow::Error>(acc + entry.compressed_size)
    })?;
    on_progress(0, total_download_bytes.max(1));

    let mut downloaded_bytes = 0u64;
    for requested in entries {
        if stop_signal.load(Ordering::Relaxed) {
            return Err(anyhow!("Download cancelled"));
        }

        let entry = parsed_entries
            .get(requested.source_path)
            .ok_or_else(|| anyhow!("Missing '{}' in remote ZIP", requested.source_path))?;
        let data_offset = read_entry_data_offset(url, entry.local_header_offset, stop_signal)?;
        let compressed_bytes = fetch_range_with_progress(
            url,
            &format!(
                "bytes={}-{}",
                data_offset,
                data_offset + entry.compressed_size.saturating_sub(1)
            ),
            stop_signal,
            |bytes_read| {
                downloaded_bytes += bytes_read as u64;
                on_progress(
                    downloaded_bytes.min(total_download_bytes),
                    total_download_bytes.max(1),
                );
            },
        )?;

        write_downloaded_entry(
            dest_dir,
            requested.dest_name,
            entry.compression_method,
            &compressed_bytes,
            entry.uncompressed_size,
            stop_signal,
        )?;
    }

    Ok(())
}
