//! Ordered history persistence. Slow media and database I/O never holds the UI cache lock.

use super::{
    HistoryAction, HistoryItem, HistoryType, MediaProof, MediaRef, history_media_leaf, notify_ui,
    run_history_action, save_db_at,
};
use chrono::Local;
use image::{ImageEncoder, RgbaImage};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, DELETE, FILE_DISPOSITION_INFO, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_GENERIC_READ, FILE_GENERIC_WRITE, FileDispositionInfo, GetFileInformationByHandle,
    SetFileInformationByHandle,
};

#[path = "transactions.rs"]
mod transactions;

static MEDIA_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct CommitContext<'a> {
    cache: &'a Mutex<Vec<HistoryItem>>,
    epoch: &'a AtomicU64,
    max_items: usize,
    media_dir: &'a Path,
    failed: &'a AtomicBool,
}

struct SaveIntent {
    item_type: HistoryType,
    text: String,
    token: Option<Arc<crate::overlay::result::ChainCancelToken>>,
    clear_epoch: u64,
}

struct PendingMedia {
    file: File,
    filename: String,
    committed: bool,
    proof: Option<MediaProof>,
}

impl PendingMedia {
    fn create(directory: &Path, prefix: &str, extension: &str) -> io::Result<Self> {
        for _ in 0..128 {
            let id = MEDIA_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let filename = format!(
                "{prefix}_{}_{}_{}.{}",
                Local::now().format("%Y%m%d_%H%M%S_%f"),
                std::process::id(),
                id,
                extension
            );
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .access_mode(FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0 | DELETE.0)
                .share_mode(0)
                .open(directory.join(&filename))
            {
                Ok(file) => {
                    return Ok(Self {
                        file,
                        filename,
                        committed: false,
                        proof: None,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate a history media file",
        ))
    }
}

impl Drop for PendingMedia {
    fn drop(&mut self) {
        if !self.committed {
            let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
            let result = unsafe {
                SetFileInformationByHandle(
                    HANDLE(self.file.as_raw_handle()),
                    FileDispositionInfo,
                    std::ptr::from_ref(&disposition).cast(),
                    std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
                )
            };
            if let Err(error) = result {
                crate::log_info!("[history] failed to retire an uncommitted media file: {error}");
            }
        }
    }
}

struct HashingWriter<'a> {
    file: &'a mut File,
    hash: Sha256,
    bytes: u64,
}

impl Write for HashingWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let written = self.file.write(bytes)?;
        self.hash.update(&bytes[..written]);
        self.bytes = self.bytes.saturating_add(written as u64);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

fn file_identity(file: &File) -> io::Result<String> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info) }
        .map_err(io::Error::other)?;
    Ok(format!(
        "{:08x}:{:08x}{:08x}",
        info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
    ))
}

fn write_proved(
    media: &mut PendingMedia,
    write: impl FnOnce(&mut HashingWriter<'_>) -> io::Result<()>,
) -> io::Result<()> {
    let mut writer = HashingWriter {
        file: &mut media.file,
        hash: Sha256::new(),
        bytes: 0,
    };
    write(&mut writer)?;
    let bytes = writer.bytes;
    let sha256 = format!("{:x}", writer.hash.finalize());
    media.file.sync_all()?;
    media.proof = Some(MediaProof {
        file_identity: file_identity(&media.file)?,
        bytes,
        sha256,
    });
    Ok(())
}

pub(super) fn process_queue(
    rx: Receiver<HistoryAction>,
    cache: Arc<Mutex<Vec<HistoryItem>>>,
    max_items: usize,
    epoch: Arc<AtomicU64>,
    failed: Arc<AtomicBool>,
    db_path: PathBuf,
    media_dir: PathBuf,
) {
    process_queue_with(rx, cache, max_items, epoch, &media_dir, failed, |items| {
        save_db_at(&db_path, items)
    });
}

fn process_queue_with(
    rx: Receiver<HistoryAction>,
    cache: Arc<Mutex<Vec<HistoryItem>>>,
    mut max_items: usize,
    epoch: Arc<AtomicU64>,
    media_dir: &Path,
    failed: Arc<AtomicBool>,
    mut persist: impl FnMut(&[HistoryItem]) -> bool,
) {
    max_items = max_items.clamp(10, 200);
    prune_with(&cache, max_items, media_dir, &failed, &mut persist);
    while let Ok(action) = rx.recv() {
        let cancelled = match &action {
            HistoryAction::SaveImage { cancel_token, .. }
            | HistoryAction::SaveAudio { cancel_token, .. }
            | HistoryAction::SaveText { cancel_token, .. } => cancel_token
                .as_ref()
                .is_some_and(|token| token.is_cancelled()),
            _ => false,
        };
        if cancelled {
            continue;
        }
        let context = CommitContext {
            cache: &cache,
            epoch: &epoch,
            max_items,
            media_dir,
            failed: &failed,
        };
        match action {
            HistoryAction::SaveImage {
                img,
                text,
                cancel_token,
                clear_epoch,
            } => {
                let media = write_image(media_dir, &img);
                commit_saved_with(
                    media,
                    SaveIntent {
                        item_type: HistoryType::Image,
                        text,
                        token: cancel_token,
                        clear_epoch,
                    },
                    &context,
                    &mut persist,
                );
            }
            HistoryAction::SaveAudio {
                wav_data,
                text,
                cancel_token,
                clear_epoch,
            } => {
                let media = if wav_data.is_empty() {
                    Ok(None)
                } else {
                    write_bytes(media_dir, "audio", "wav", &wav_data).map(Some)
                };
                commit_saved_with(
                    media,
                    SaveIntent {
                        item_type: HistoryType::Audio,
                        text,
                        token: cancel_token,
                        clear_epoch,
                    },
                    &context,
                    &mut persist,
                );
            }
            HistoryAction::SaveText {
                result_text,
                input_text,
                cancel_token,
                clear_epoch,
            } => {
                let media = write_bytes(media_dir, "text", "txt", input_text.as_bytes()).map(Some);
                commit_saved_with(
                    media,
                    SaveIntent {
                        item_type: HistoryType::Text,
                        text: result_text,
                        token: cancel_token,
                        clear_epoch,
                    },
                    &context,
                    &mut persist,
                );
            }
            HistoryAction::Delete { item } => {
                if let Some(change) = transactions::delete(&cache, &item)
                    && let Some(removed) =
                        transactions::publish_with(&cache, &failed, change, &mut persist, || {})
                {
                    remove_unreferenced(media_dir, &cache, &removed);
                }
            }
            HistoryAction::ClearAll => {
                let change = transactions::clear(&cache);
                if let Some(removed) =
                    transactions::publish_with(&cache, &failed, change, &mut persist, || {})
                {
                    remove_unreferenced(media_dir, &cache, &removed);
                }
            }
            HistoryAction::Prune(limit) => {
                max_items = limit.clamp(10, 200);
                prune_with(&cache, max_items, media_dir, &failed, &mut persist);
            }
        }
    }
}

fn write_image(directory: &Path, image: &RgbaImage) -> io::Result<Option<PendingMedia>> {
    let mut media = PendingMedia::create(directory, "img", "png")?;
    write_proved(&mut media, |writer| {
        image::codecs::png::PngEncoder::new(writer)
            .write_image(
                image.as_raw(),
                image.width(),
                image.height(),
                image::ExtendedColorType::Rgba8,
            )
            .map_err(io::Error::other)
    })?;
    Ok(Some(media))
}

fn write_bytes(
    directory: &Path,
    prefix: &str,
    extension: &str,
    bytes: &[u8],
) -> io::Result<PendingMedia> {
    let mut media = PendingMedia::create(directory, prefix, extension)?;
    write_proved(&mut media, |writer| writer.write_all(bytes))?;
    Ok(media)
}

fn commit_saved_with(
    media: io::Result<Option<PendingMedia>>,
    intent: SaveIntent,
    context: &CommitContext<'_>,
    mut persist: impl FnMut(&[HistoryItem]) -> bool,
) {
    let Ok(mut media) = media else {
        context.failed.store(true, Ordering::Release);
        notify_ui();
        return;
    };
    let now = Local::now();
    let item = HistoryItem {
        id: now.timestamp_nanos_opt().unwrap_or(0),
        timestamp: now.format("%Y-%m-%d %H:%M:%S").to_string(),
        item_type: intent.item_type,
        text: intent.text,
        media_path: media
            .as_ref()
            .map_or_else(String::new, |file| file.filename.clone()),
        media_proof: media.as_ref().and_then(|file| file.proof.clone()),
    };
    let Some(change) = transactions::insert(item, intent.token, intent.clear_epoch, context) else {
        return;
    };
    if let Some(removed) = transactions::publish_with(
        context.cache,
        context.failed,
        change,
        &mut persist,
        move || {
            if let Some(media) = &mut media {
                media.committed = true;
            }
            drop(media);
        },
    ) {
        remove_unreferenced(context.media_dir, context.cache, &removed);
    }
}

fn prune_with(
    cache: &Mutex<Vec<HistoryItem>>,
    limit: usize,
    directory: &Path,
    failed: &AtomicBool,
    persist: impl FnMut(&[HistoryItem]) -> bool,
) {
    if let Some(change) = transactions::prune(cache, limit)
        && let Some(removed) = transactions::publish_with(cache, failed, change, persist, || {})
    {
        remove_unreferenced(directory, cache, &removed);
    }
}

fn remove_unreferenced(directory: &Path, cache: &Mutex<Vec<HistoryItem>>, paths: &[MediaRef]) {
    for media in paths {
        let filename = &media.filename;
        if !history_media_leaf(filename) {
            continue;
        }
        if cache
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .any(|item| item.media_path == *filename)
        {
            continue;
        }
        if let Some(proof) = &media.proof {
            let _ = remove_verified_file(&directory.join(filename), proof);
        }
    }
}

fn remove_verified_file(path: &Path, proof: &MediaProof) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .read(true)
        .access_mode(FILE_GENERIC_READ.0 | DELETE.0)
        .share_mode(0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.file_attributes() & 0x400 != 0
        || metadata.len() != proof.bytes
        || file_identity(&file)? != proof.file_identity
    {
        return Err(io::Error::other("history media identity changed"));
    }
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    if !format!("{:x}", hash.finalize()).eq_ignore_ascii_case(&proof.sha256) {
        return Err(io::Error::other("history media content changed"));
    }
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfo,
            std::ptr::from_ref(&disposition).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    }
    .map_err(io::Error::other)
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "transaction_tests.rs"]
mod transaction_tests;

#[cfg(test)]
#[path = "media_creation_tests.rs"]
mod media_creation_tests;

#[cfg(test)]
#[path = "state_machine_tests.rs"]
mod state_machine_tests;
