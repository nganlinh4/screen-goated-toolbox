//! Recoverable conversation records and a derived search index.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, ensure};
use chrono::Local;

use super::{ConvMeta, SavedFrame, Transcript, derive_snippet, derive_title};

mod files;
mod locking;
#[cfg(test)]
mod tests;

pub(super) struct Image {
    pub jpeg: Vec<u8>,
    pub turn_id: u64,
    pub source: serde_json::Value,
}

/// Advanced by every `Store::clear`. A save admitted under an earlier
/// generation never publishes, so a save already in flight when the user
/// clears memory cannot write its conversation back.
static CLEAR_GENERATION: AtomicU64 = AtomicU64::new(0);

pub(super) fn clear_generation() -> u64 {
    CLEAR_GENERATION.load(Ordering::SeqCst)
}

pub(super) struct Store {
    dir: PathBuf,
    max_items: usize,
    admitted_generation: Option<u64>,
}

#[derive(Default)]
struct PendingImages(Vec<PathBuf>);

impl Drop for PendingImages {
    fn drop(&mut self) {
        for path in &self.0 {
            if let Err(error) = fs::remove_file(path) {
                cc_diagnostic!("[cc-mem] incomplete image cleanup deferred: {error}");
            }
        }
    }
}

impl Store {
    pub(super) fn new(dir: PathBuf, max_items: usize) -> Self {
        Self {
            dir,
            max_items: max_items.max(1),
            admitted_generation: None,
        }
    }

    /// Refuse `create` once the store has been cleared after `generation`.
    pub(super) fn admitted_at(mut self, generation: u64) -> Self {
        self.admitted_generation = Some(generation);
        self
    }

    /// Delete every conversation, image, index and preserved copy. A pending
    /// embedding then finds no record, and an earlier-admitted save is refused.
    pub(super) fn clear(&self) -> anyhow::Result<()> {
        let _guard = locking::acquire(&self.dir)?;
        CLEAR_GENERATION.fetch_add(1, Ordering::SeqCst);
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            // The lock stays held (and open) until the store is empty.
            if entry.file_name() == locking::LOCK_FILE || !files::regular_file(&path)? {
                continue;
            }
            fs::remove_file(path)?;
        }
        Ok(())
    }

    pub(super) fn snapshot(&self) -> anyhow::Result<Vec<ConvMeta>> {
        let _guard = locking::acquire(&self.dir)?;
        self.reconcile()
    }

    pub(super) fn open(&self, id: i64) -> anyhow::Result<Option<String>> {
        let _guard = locking::acquire(&self.dir)?;
        if !self.reconcile()?.iter().any(|meta| meta.id == id) {
            return Ok(None);
        }
        Ok(Some(
            files::read_record(&self.record_path(id))?.lines.join("\n"),
        ))
    }

    pub(super) fn create(&self, lines: &[String], images: &[Image]) -> anyhow::Result<i64> {
        let _guard = locking::acquire(&self.dir)?;
        ensure!(
            self.admitted_generation
                .is_none_or(|generation| generation == clear_generation()),
            "conversation memory was cleared after this save was admitted"
        );
        let index = self.reconcile()?;
        let now = Local::now();
        let mut previous = index
            .iter()
            .map(|meta| meta.id)
            .max()
            .unwrap_or_default()
            .max(0);
        for entry in fs::read_dir(&self.dir)? {
            let name = entry?.file_name();
            if let Some(id) = files::record_id(&name, "conv_")
                .or_else(|| files::record_id(&name, "retired_"))
                .or_else(|| files::image_id(&name))
            {
                previous = previous.max(id);
            }
        }
        let id = now
            .timestamp_nanos_opt()
            .context("conversation timestamp is out of range")?
            .max(
                previous
                    .checked_add(1)
                    .context("conversation identity exhausted")?,
            );
        ensure!(
            id > 0 && !self.record_path(id).exists(),
            "conversation identity is unavailable"
        );
        // Never replace a leftover image from an interrupted publication.
        for slot in 0..images.len().min(super::MAX_EMBED_IMAGES) {
            ensure!(
                !self.dir.join(format!("img_{id}_{slot}.jpg")).exists(),
                "conversation image identity is unavailable"
            );
        }
        let mut record = Transcript {
            lines: lines.to_vec(),
            frames: Vec::new(),
            version: 1,
            metadata: Some(ConvMeta {
                id,
                timestamp: now.format("%Y-%m-%d %H:%M:%S").to_string(),
                title: derive_title(lines),
                snippet: derive_snippet(lines),
                images: Vec::new(),
                embedding: Vec::new(),
            }),
        };
        // Check the immutable transcript before publishing any companion image.
        files::check_json(&record)?;
        let mut pending = PendingImages::default();
        for (slot, image) in images.iter().take(super::MAX_EMBED_IMAGES).enumerate() {
            let name = format!("img_{id}_{slot}.jpg");
            let path = self.dir.join(&name);
            crate::atomic_json::write_bytes_atomic(&path, &image.jpeg)?;
            pending.0.push(path);
            record.frames.push(SavedFrame {
                image: name.clone(),
                turn_id: image.turn_id,
                source: image.source.clone(),
            });
            record.metadata.as_mut().unwrap().images.push(name);
        }
        let mut candidate_index = index;
        candidate_index.insert(0, record.metadata.as_ref().unwrap().clone());
        candidate_index.truncate(self.max_items);
        files::check_json(&candidate_index)?;
        files::write_record(&self.record_path(id), &record)?;
        pending.0.clear();
        self.reconcile()?;
        Ok(id)
    }

    pub(super) fn set_embedding(&self, id: i64, embedding: Vec<f32>) -> anyhow::Result<()> {
        ensure!(
            embedding.iter().all(|value| value.is_finite()),
            "invalid conversation embedding"
        );
        let _guard = locking::acquire(&self.dir)?;
        let mut index = self.reconcile()?;
        let Some(meta) = index.iter_mut().find(|meta| meta.id == id) else {
            return Ok(()); // A slow provider must not resurrect a pruned record.
        };
        meta.embedding.clone_from(&embedding);
        files::check_json(&index)?;
        let path = self.record_path(id);
        let mut record = files::read_record(&path)?;
        record
            .metadata
            .as_mut()
            .context("conversation metadata is unavailable")?
            .embedding = embedding;
        files::write_record(&path, &record)?;
        self.reconcile()?;
        Ok(())
    }

    fn record_path(&self, id: i64) -> PathBuf {
        self.dir.join(format!("conv_{id}.json"))
    }

    fn reconcile(&self) -> anyhow::Result<Vec<ConvMeta>> {
        self.cleanup_retired()?;
        let index_path = self.dir.join("index.json");
        let cached = files::read_index(&index_path)?;
        let mut records = Vec::new();
        let mut metadata_bytes = 0usize;
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let Some(id) = files::record_id(&entry.file_name(), "conv_") else {
                continue;
            };
            let path = entry.path();
            let Some(mut record) = files::recover_record(&path)? else {
                continue;
            };
            ensure!(
                record.version <= 1,
                "unsupported conversation memory version"
            );
            if record.version == 0 {
                let old = cached
                    .as_ref()
                    .and_then(|items| items.iter().find(|meta| meta.id == id));
                record.metadata = Some(self.legacy_metadata(id, &record, old)?);
                record.version = 1;
                files::write_record(&path, &record)?;
            }
            let Some(meta) = record.metadata.as_ref().filter(|meta| meta.id == id) else {
                files::preserve(&path)?;
                continue;
            };
            let mut meta = meta.clone();
            meta.images.retain(|name| files::owned_image(id, name));
            metadata_bytes = metadata_bytes
                .checked_add(serde_json::to_vec_pretty(&meta)?.len())
                .context("conversation index size overflow")?;
            ensure!(
                metadata_bytes <= files::MAX_JSON_BYTES,
                "conversation index exceeds its recovery limit"
            );
            records.push(meta);
        }
        records.sort_by_key(|meta| std::cmp::Reverse(meta.id));
        for meta in records.iter().skip(self.max_items) {
            let retired = self.dir.join(format!("retired_{}.json", meta.id));
            ensure!(
                !retired.exists(),
                "conversation retirement identity is unavailable"
            );
            if let Err(error) = fs::rename(self.record_path(meta.id), retired) {
                cc_diagnostic!("[cc-mem] conversation retirement deferred: {error}");
            }
        }
        records.truncate(self.max_items);
        let index = records;
        if cached.as_ref() != Some(&index) {
            files::check_json(&index)?;
            crate::atomic_json::write_json_atomic(&index_path, &index)?;
        }
        self.cleanup_retired()?;
        Ok(index)
    }

    fn legacy_metadata(
        &self,
        id: i64,
        record: &Transcript,
        old: Option<&ConvMeta>,
    ) -> anyhow::Result<ConvMeta> {
        let mut images = Vec::new();
        for slot in 0..super::MAX_EMBED_IMAGES {
            let name = format!("img_{id}_{slot}.jpg");
            if files::regular_file(&self.dir.join(&name))? {
                images.push(name);
            }
        }
        Ok(ConvMeta {
            id,
            timestamp: old.map(|meta| meta.timestamp.clone()).unwrap_or_else(|| {
                chrono::DateTime::from_timestamp_nanos(id)
                    .with_timezone(&Local)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string()
            }),
            title: derive_title(&record.lines),
            snippet: derive_snippet(&record.lines),
            images,
            embedding: old.map(|meta| meta.embedding.clone()).unwrap_or_default(),
        })
    }

    fn cleanup_retired(&self) -> anyhow::Result<()> {
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let Some(id) = files::record_id(&entry.file_name(), "retired_") else {
                continue;
            };
            let path = entry.path();
            let cleanup = (|| -> anyhow::Result<()> {
                let Some(record) = files::recover_record(&path)? else {
                    return Ok(());
                };
                let Some(meta) = record
                    .metadata
                    .filter(|meta| record.version == 1 && meta.id == id)
                else {
                    return Ok(());
                };
                for image in meta
                    .images
                    .iter()
                    .filter(|name| files::owned_image(id, name))
                {
                    let image = self.dir.join(image);
                    if files::regular_file(&image)? {
                        fs::remove_file(image)?;
                    }
                }
                fs::remove_file(&path)?;
                Ok(())
            })();
            if let Err(error) = cleanup {
                cc_diagnostic!("[cc-mem] retired conversation cleanup deferred: {error}");
            }
        }
        Ok(())
    }
}
