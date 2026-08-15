#[cfg(debug_assertions)]
mod debug {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{LazyLock, Mutex};
    use std::time::Duration;

    use anyhow::{Context, Result};
    use image::codecs::jpeg::JpegEncoder;
    use image::{ExtendedColorType, ImageEncoder as _};
    use serde::Serialize;

    use super::super::contract::{DetectedTextRegion, SemanticRole, TranslationDocument};
    use super::super::evidence_capture::capture_stable_selection;
    use super::super::geometry::{PixelRegion, normalized_region};
    use crate::overlay::selection::CapturedRegion;

    const MAX_RUNS: usize = 24;
    const MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
    const RESULT_PAINT_TIMEOUT: Duration = Duration::from_secs(3);
    static FINALIZE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    pub(crate) struct RunEvidence {
        state: Option<State>,
    }

    struct State {
        directory: PathBuf,
        runs_root: PathBuf,
        trace_id: String,
        created_at: String,
        selection: Selection,
        target_language: String,
        configured_model: String,
        translation_prompt: String,
        source_jpeg: Vec<u8>,
        candidates: Vec<DetectedTextRegion>,
    }

    #[derive(Clone, Copy, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Selection {
        left: i32,
        top: i32,
        width: u32,
        height: u32,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct RunRecord {
        version: u8,
        trace_id: String,
        created_at: String,
        status: String,
        failed_stage: Option<String>,
        error: Option<String>,
        selection: Selection,
        target_language: String,
        configured_model: String,
        translation_prompt: String,
        rendered_region_count: usize,
        result_capture: String,
        regions: Vec<RegionRecord>,
        timings_ms: Vec<TimingRecord>,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct RegionRecord {
        id: u16,
        normalized_box_2d: [u16; 4],
        pixel_box: [u32; 4],
        ocr_candidates: Vec<String>,
        selected_source_text: Option<String>,
        translated_text: Option<String>,
        group_member_ids: Option<Vec<u16>>,
        semantic_role: Option<SemanticRole>,
        visual_style: Option<super::super::appearance::VisualSignature>,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct TimingRecord {
        phase: &'static str,
        elapsed_ms: f64,
    }

    impl RunEvidence {
        pub(crate) fn begin(
            trace_id: &str,
            capture: &CapturedRegion,
            source_jpeg: &[u8],
            target_language: &str,
            configured_model: &str,
            translation_prompt: &str,
        ) -> Self {
            let Some(runs_root) = evidence_root() else {
                return Self { state: None };
            };
            let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S-%3f");
            let directory = runs_root.join(format!("{stamp}-{trace_id}"));
            if let Err(error) = std::fs::create_dir_all(&directory) {
                crate::log_info!("[Screen Translate] evidence setup failed: {error}");
                return Self { state: None };
            }
            let selection = Selection {
                left: capture.left,
                top: capture.top,
                width: capture.width,
                height: capture.height,
            };
            let source_jpeg = source_jpeg.to_vec();
            spawn_write(directory.join("source.jpg"), source_jpeg.clone());
            crate::log_info!(
                "[Screen Translate] trace={trace_id} evidence={}",
                directory.display()
            );
            Self {
                state: Some(State {
                    directory,
                    runs_root,
                    trace_id: trace_id.to_string(),
                    created_at: chrono::Local::now().to_rfc3339(),
                    selection,
                    target_language: target_language.to_string(),
                    configured_model: configured_model.to_string(),
                    translation_prompt: translation_prompt.to_string(),
                    source_jpeg,
                    candidates: Vec::new(),
                }),
            }
        }

        pub(crate) fn detected(&mut self, candidates: &[DetectedTextRegion]) {
            let Some(state) = self.state.as_mut() else {
                return;
            };
            state.candidates = candidates.to_vec();
            let source = state.source_jpeg.clone();
            let candidates = state.candidates.clone();
            let size = (state.selection.width, state.selection.height);
            let path = state.directory.join("detector.jpg");
            std::thread::Builder::new()
                .name("sgt-screen-translate-evidence-detector".to_string())
                .spawn(move || {
                    if let Err(error) = save_detector_preview(&path, &source, &candidates, size) {
                        crate::log_info!("[Screen Translate] detector evidence failed: {error:#}");
                    }
                })
                .ok();
        }

        pub(crate) fn finish(mut self, document: TranslationDocument, rendered_count: usize) {
            if let Some(state) = self.state.take() {
                finalize(
                    state,
                    "complete",
                    None,
                    None,
                    Some(document),
                    rendered_count,
                    true,
                );
            }
        }

        pub(crate) fn no_text(mut self) {
            if let Some(state) = self.state.take() {
                finalize(state, "no_text", None, None, None, 0, false);
            }
        }

        pub(crate) fn fail(mut self, stage: &str, error: &anyhow::Error) {
            if let Some(state) = self.state.take() {
                let status = if error.to_string().contains("cancelled") {
                    "cancelled"
                } else {
                    "error"
                };
                finalize(
                    state,
                    status,
                    Some(stage.to_string()),
                    Some(format!("{error:#}")),
                    None,
                    0,
                    false,
                );
            }
        }
    }

    impl Drop for RunEvidence {
        fn drop(&mut self) {
            if let Some(state) = self.state.take() {
                finalize(
                    state,
                    "cancelled",
                    Some("pipeline".to_string()),
                    None,
                    None,
                    0,
                    false,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn finalize(
        state: State,
        status: &str,
        failed_stage: Option<String>,
        error: Option<String>,
        document: Option<TranslationDocument>,
        rendered_count: usize,
        capture_result: bool,
    ) {
        let status = status.to_string();
        std::thread::Builder::new()
            .name("sgt-screen-translate-evidence-finalize".to_string())
            .spawn(move || {
                let _guard = FINALIZE_LOCK.lock().unwrap();
                let result_capture = if capture_result {
                    capture_result_image(&state, rendered_count)
                        .unwrap_or_else(|error| format!("error: {error:#}"))
                } else {
                    "not_applicable".to_string()
                };
                let record = build_record(
                    &state,
                    status,
                    failed_stage,
                    error,
                    document.as_ref(),
                    rendered_count,
                    result_capture,
                );
                if let Err(error) = write_record(&state.directory, &record) {
                    crate::log_info!("[Screen Translate] evidence manifest failed: {error:#}");
                }
                if let Err(error) = prune_runs(&state.runs_root, &state.directory) {
                    crate::log_info!("[Screen Translate] evidence pruning failed: {error:#}");
                }
            })
            .ok();
    }

    fn build_record(
        state: &State,
        status: String,
        failed_stage: Option<String>,
        error: Option<String>,
        document: Option<&TranslationDocument>,
        rendered_count: usize,
        result_capture: String,
    ) -> RunRecord {
        let translations = document
            .map(|document| {
                document
                    .regions
                    .iter()
                    .flat_map(|region| {
                        region
                            .selections
                            .iter()
                            .map(move |selection| (selection.region_id, (region, selection)))
                    })
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        let regions = state
            .candidates
            .iter()
            .map(|candidate| {
                let pixels = normalized_region(
                    candidate.bounds,
                    state.selection.width,
                    state.selection.height,
                );
                let translated = translations.get(&candidate.id).copied();
                RegionRecord {
                    id: candidate.id,
                    normalized_box_2d: candidate.bounds.into(),
                    pixel_box: [pixels.x, pixels.y, pixels.width, pixels.height],
                    ocr_candidates: candidate.source_alternatives.clone(),
                    selected_source_text: translated
                        .map(|(_, selection)| selection.source_text.clone()),
                    translated_text: translated.map(|(region, _)| region.translated_text.clone()),
                    group_member_ids: translated.map(|(region, _)| region.member_ids.clone()),
                    semantic_role: translated.map(|(region, _)| region.semantic_role),
                    visual_style: candidate.appearance,
                }
            })
            .collect();
        let timings_ms = crate::overlay::result::latency::snapshot(&state.trace_id)
            .into_iter()
            .map(|(phase, elapsed_ms)| TimingRecord { phase, elapsed_ms })
            .collect();
        RunRecord {
            version: 1,
            trace_id: state.trace_id.clone(),
            created_at: state.created_at.clone(),
            status,
            failed_stage,
            error,
            selection: state.selection,
            target_language: state.target_language.clone(),
            configured_model: state.configured_model.clone(),
            translation_prompt: state.translation_prompt.clone(),
            rendered_region_count: rendered_count,
            result_capture,
            regions,
            timings_ms,
        }
    }

    fn capture_result_image(state: &State, rendered_count: usize) -> Result<String> {
        let painted_count = crate::overlay::result::latency::wait_for_window_phase_count(
            &state.trace_id,
            "final_painted",
            rendered_count,
            RESULT_PAINT_TIMEOUT,
        );
        if painted_count >= rendered_count {
            crate::overlay::result::latency::mark(&state.trace_id, "evidence_all_regions_painted");
        }
        let (image, visually_stable) = capture_stable_selection((
            state.selection.left,
            state.selection.top,
            state.selection.width,
            state.selection.height,
        ))?;
        crate::overlay::result::latency::mark(&state.trace_id, "evidence_visual_capture_ready");
        save_jpeg(&state.directory.join("result.jpg"), &image)?;
        let paint_status = if painted_count >= rendered_count {
            "all_regions_painted".to_string()
        } else {
            format!("paint_timeout_{painted_count}_of_{rendered_count}")
        };
        let visual_status = if visually_stable {
            "visually_stable"
        } else {
            "visual_stability_timeout"
        };
        Ok(format!("saved_after_{paint_status}_{visual_status}"))
    }

    fn save_detector_preview(
        path: &Path,
        source_jpeg: &[u8],
        candidates: &[DetectedTextRegion],
        size: (u32, u32),
    ) -> Result<()> {
        let mut image = image::load_from_memory(source_jpeg)
            .context("decode detector evidence source")?
            .to_rgba8();
        for candidate in candidates {
            draw_box(
                &mut image,
                normalized_region(candidate.bounds, size.0, size.1),
            );
        }
        save_jpeg(path, &image)
    }

    fn draw_box(image: &mut image::RgbaImage, region: PixelRegion) {
        if region.width == 0 || region.height == 0 || image.width() == 0 || image.height() == 0 {
            return;
        }
        let left = region.x.min(image.width() - 1);
        let top = region.y.min(image.height() - 1);
        let right = region
            .x
            .saturating_add(region.width.saturating_sub(1))
            .min(image.width() - 1);
        let bottom = region
            .y
            .saturating_add(region.height.saturating_sub(1))
            .min(image.height() - 1);
        for inset in 0..3_u32 {
            let x1 = left.saturating_add(inset).min(right);
            let x2 = right.saturating_sub(inset).max(left);
            let y1 = top.saturating_add(inset).min(bottom);
            let y2 = bottom.saturating_sub(inset).max(top);
            for x in x1..=x2 {
                image.put_pixel(x, y1, image::Rgba([255, 40, 80, 255]));
                image.put_pixel(x, y2, image::Rgba([255, 40, 80, 255]));
            }
            for y in y1..=y2 {
                image.put_pixel(x1, y, image::Rgba([255, 40, 80, 255]));
                image.put_pixel(x2, y, image::Rgba([255, 40, 80, 255]));
            }
        }
    }

    fn save_jpeg(path: &Path, image: &image::RgbaImage) -> Result<()> {
        let rgb = image::DynamicImage::ImageRgba8(image.clone()).to_rgb8();
        let file = std::fs::File::create(path)?;
        JpegEncoder::new_with_quality(file, 88).write_image(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            ExtendedColorType::Rgb8,
        )?;
        Ok(())
    }

    fn write_record(directory: &Path, record: &RunRecord) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(record)?;
        let temporary = directory.join("run.json.tmp");
        std::fs::write(&temporary, bytes)?;
        std::fs::rename(temporary, directory.join("run.json"))?;
        Ok(())
    }

    fn spawn_write(path: PathBuf, bytes: Vec<u8>) {
        std::thread::Builder::new()
            .name("sgt-screen-translate-evidence-source".to_string())
            .spawn(move || {
                if let Err(error) = std::fs::write(path, bytes) {
                    crate::log_info!("[Screen Translate] source evidence failed: {error}");
                }
            })
            .ok();
    }

    fn evidence_root() -> Option<PathBuf> {
        let cache = PathBuf::from(std::env::var_os("SGT_DEV_CACHE_ROOT")?);
        if !cache.is_absolute() {
            crate::log_info!("[Screen Translate] ignored non-absolute development cache root");
            return None;
        }
        Some(cache.join("evidence").join("screen-translate").join("runs"))
    }

    fn prune_runs(root: &Path, protected: &Path) -> Result<()> {
        let mut runs = std::fs::read_dir(root)?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let path = entry.path();
                (entry.file_type().ok()?.is_dir() && path.parent() == Some(root)).then_some(path)
            })
            .map(|path| {
                let modified = std::fs::metadata(&path)
                    .and_then(|metadata| metadata.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                let bytes = directory_bytes(&path);
                (path, modified, bytes)
            })
            .collect::<Vec<_>>();
        runs.sort_unstable_by_key(|(_, modified, _)| *modified);
        let mut total = runs.iter().map(|(_, _, bytes)| *bytes).sum::<u64>();
        let mut count = runs.len();
        for (path, _, bytes) in runs {
            if count <= MAX_RUNS && total <= MAX_TOTAL_BYTES {
                break;
            }
            if path == protected {
                continue;
            }
            std::fs::remove_dir_all(&path)?;
            count = count.saturating_sub(1);
            total = total.saturating_sub(bytes);
        }
        Ok(())
    }

    fn directory_bytes(root: &Path) -> u64 {
        let Ok(entries) = std::fs::read_dir(root) else {
            return 0;
        };
        entries
            .filter_map(|entry| entry.ok())
            .map(|entry| {
                entry
                    .file_type()
                    .ok()
                    .map(|kind| {
                        if kind.is_dir() {
                            directory_bytes(&entry.path())
                        } else if kind.is_file() {
                            entry.metadata().map(|metadata| metadata.len()).unwrap_or(0)
                        } else {
                            0
                        }
                    })
                    .unwrap_or(0)
            })
            .sum()
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::overlay::screen_translate::contract::NormalizedBounds;

        fn temporary_root(label: &str) -> PathBuf {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            std::env::temp_dir().join(format!(
                "sgt-screen-translate-{label}-{}-{nonce}",
                std::process::id()
            ))
        }

        #[test]
        fn detector_preview_paints_the_normalized_region() {
            let root = temporary_root("preview");
            std::fs::create_dir(&root).unwrap();
            let path = root.join("detector.jpg");
            let source = image::RgbImage::from_pixel(80, 60, image::Rgb([240, 240, 240]));
            let mut jpeg = Vec::new();
            JpegEncoder::new_with_quality(&mut jpeg, 95)
                .write_image(source.as_raw(), 80, 60, ExtendedColorType::Rgb8)
                .unwrap();
            let candidates = [DetectedTextRegion {
                id: 1,
                bounds: NormalizedBounds {
                    left: 250,
                    top: 250,
                    right: 750,
                    bottom: 750,
                },
                source_text: "text".to_string(),
                source_alternatives: vec!["text".to_string()],
                appearance: None,
            }];

            save_detector_preview(&path, &jpeg, &candidates, (80, 60)).unwrap();

            let preview = image::open(&path).unwrap().to_rgb8();
            let edge = preview.get_pixel(20, 15).0;
            assert!(edge[0] > 180 && edge[1] < 100 && edge[2] < 130);
            std::fs::remove_dir_all(root).unwrap();
        }

        #[test]
        fn pruning_is_bounded_and_preserves_the_active_run() {
            let root = temporary_root("prune");
            std::fs::create_dir(&root).unwrap();
            let protected = root.join("run-24");
            for index in 0..=MAX_RUNS {
                let run = root.join(format!("run-{index:02}"));
                std::fs::create_dir(&run).unwrap();
                std::fs::write(run.join("source.jpg"), [index as u8]).unwrap();
            }

            prune_runs(&root, &protected).unwrap();

            assert!(protected.is_dir());
            assert_eq!(std::fs::read_dir(&root).unwrap().count(), MAX_RUNS);
            std::fs::remove_dir_all(root).unwrap();
        }
    }
}

#[cfg(not(debug_assertions))]
mod release {
    use super::super::contract::{DetectedTextRegion, TranslationDocument};
    use crate::overlay::selection::CapturedRegion;

    pub(crate) struct RunEvidence;

    impl RunEvidence {
        pub(crate) fn begin(
            _trace_id: &str,
            _capture: &CapturedRegion,
            _source_jpeg: &[u8],
            _target_language: &str,
            _configured_model: &str,
            _translation_prompt: &str,
        ) -> Self {
            Self
        }

        pub(crate) fn detected(&mut self, _candidates: &[DetectedTextRegion]) {}
        pub(crate) fn finish(self, _document: TranslationDocument, _rendered_count: usize) {}
        pub(crate) fn no_text(self) {}
        pub(crate) fn fail(self, _stage: &str, _error: &anyhow::Error) {}
    }
}

#[cfg(debug_assertions)]
pub(super) use debug::RunEvidence;
#[cfg(not(debug_assertions))]
pub(super) use release::RunEvidence;
