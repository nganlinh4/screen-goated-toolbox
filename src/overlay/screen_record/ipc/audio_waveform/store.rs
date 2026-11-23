//! Bounded waveform cache decoding and collision-safe publication.
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;

use anyhow::{Context, Result, bail};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    DELETE, FILE_DISPOSITION_FLAG_ON_CLOSE, FILE_DISPOSITION_INFO_EX, FILE_FLAG_DELETE_ON_CLOSE,
    FILE_GENERIC_READ, FILE_GENERIC_WRITE, FileDispositionInfoEx, SetFileInformationByHandle,
};

use super::{
    QuantizedWaveformBin, SourceWaveformEnvelope, WAVEFORM_CACHE_MAGIC, WAVEFORM_CACHE_VERSION,
};

const HEADER_BYTES: u64 = 21;
const MAX_CACHE_BYTES: u64 = 64 * 1024 * 1024;

/// How the disk cache answered. A corrupt entry (a regular file that fails
/// validation, e.g. truncated by a killed worker) is replaced by the fresh
/// analysis; any other existing entry is never overwritten.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CacheRead {
    Hit,
    Missing,
    Corrupt,
}

pub(super) fn load_or_analyze(
    path: Option<&Path>,
    analyze: impl FnOnce() -> Result<SourceWaveformEnvelope, String>,
) -> Result<(SourceWaveformEnvelope, CacheRead), String> {
    let mut read = CacheRead::Missing;
    if let Some(path) = path {
        match load(path) {
            Ok(Some(envelope)) => return Ok((envelope, CacheRead::Hit)),
            Ok(None) => {}
            Err(_) if fs::symlink_metadata(path).is_ok_and(|meta| meta.is_file()) => {
                read = CacheRead::Corrupt;
            }
            Err(_) => {}
        }
    }
    analyze().map(|envelope| (envelope, read))
}

pub(super) fn load(path: &Path) -> Result<Option<SourceWaveformEnvelope>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let size = file.metadata()?.len();
    if !(HEADER_BYTES..=MAX_CACHE_BYTES).contains(&size) {
        bail!("invalid waveform cache size");
    }
    let mut reader = BufReader::new(file);
    let mut magic = [0; 5];
    std::io::Read::read_exact(&mut reader, &mut magic)?;
    if &magic != WAVEFORM_CACHE_MAGIC
        || reader.read_u32::<LittleEndian>()? != WAVEFORM_CACHE_VERSION
    {
        return Ok(None);
    }
    let duration = reader.read_f64::<LittleEndian>()?;
    let count = reader.read_u32::<LittleEndian>()? as usize;
    let expected_size = (count as u64)
        .checked_mul(4)
        .and_then(|n| n.checked_add(HEADER_BYTES));
    if !duration.is_finite() || duration < 0.0 || expected_size != Some(size) {
        bail!("invalid waveform cache header");
    }
    let expected_bins = duration * f64::from(super::SOURCE_BINS_PER_SECOND);
    if !expected_bins.is_finite() || (expected_bins - count as f64).abs() > 1.01 {
        bail!("inconsistent waveform cache duration");
    }
    let mut bins = Vec::new();
    bins.try_reserve_exact(count)
        .context("allocate waveform cache")?;
    for _ in 0..count {
        let min = reader.read_i16::<LittleEndian>()?;
        let max = reader.read_i16::<LittleEndian>()?;
        if min > max {
            bail!("invalid waveform cache bin");
        }
        bins.push(QuantizedWaveformBin { min, max });
    }
    Ok(Some(SourceWaveformEnvelope {
        source_duration_sec: duration,
        bins,
    }))
}

pub(super) fn save(
    path: &Path,
    envelope: &SourceWaveformEnvelope,
    replace_corrupt: bool,
) -> Result<()> {
    let parent = path.parent().context("waveform cache has no parent")?;
    fs::create_dir_all(parent)?;
    let count = u32::try_from(envelope.bins.len())?;
    if u64::from(count) * 4 + HEADER_BYTES > MAX_CACHE_BYTES {
        bail!("waveform cache exceeds size limit");
    }
    let _admission =
        super::super::audio_waveform_cache::admit(parent, u64::from(count) * 4 + HEADER_BYTES)?;
    let mut nonce = [0_u8; 8];
    getrandom::fill(&mut nonce)
        .map_err(|error| anyhow::anyhow!("waveform scratch identity: {error}"))?;
    let scratch = path.with_extension(format!("waveform-{:016x}.part", u64::from_le_bytes(nonce)));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .access_mode(FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0 | DELETE.0)
        .share_mode(0)
        .custom_flags(FILE_FLAG_DELETE_ON_CLOSE.0)
        .open(scratch)?;
    {
        let mut writer = BufWriter::new(&mut file);
        writer.write_all(WAVEFORM_CACHE_MAGIC)?;
        writer.write_u32::<LittleEndian>(WAVEFORM_CACHE_VERSION)?;
        writer.write_f64::<LittleEndian>(envelope.source_duration_sec)?;
        writer.write_u32::<LittleEndian>(count)?;
        for bin in &envelope.bins {
            writer.write_i16::<LittleEndian>(bin.min)?;
            writer.write_i16::<LittleEndian>(bin.max)?;
        }
        writer.flush()?;
    }
    file.sync_all()?;
    publish(&file, path, replace_corrupt)?;
    Ok(())
}

fn publish(file: &File, path: &Path, replace_corrupt: bool) -> Result<()> {
    let parent = path
        .parent()
        .context("waveform cache has no parent")?
        .canonicalize()?;
    let destination = parent.join(path.file_name().context("waveform cache has no name")?);
    let wide: Vec<u16> = destination.as_os_str().encode_wide().collect();
    crate::verified_scratch::rename_file_by_handle(file, &wide, replace_corrupt)?;
    unsafe {
        let disposition = FILE_DISPOSITION_INFO_EX {
            Flags: FILE_DISPOSITION_FLAG_ON_CLOSE,
        };
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfoEx,
            std::ptr::from_ref(&disposition).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFO_EX>() as u32,
        )?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
