use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use image::ExtendedColorType;
use image::codecs::jpeg::JpegEncoder;
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetSystemMetrics, GetWindowRect, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

use crate::overlay::selection::CapturedRegion;

pub(super) fn start_foreground() {
    if crate::overlay::is_busy() {
        return;
    }
    crate::overlay::set_is_busy(true);
    let (job_id, cancel) = super::runtime::begin_job();
    std::thread::spawn(move || {
        let result = capture_foreground()
            .and_then(|region| translate_region(job_id, Arc::clone(&cancel), region));
        if let Err(error) = result
            && super::runtime::is_current(job_id)
            && !cancel.load(Ordering::SeqCst)
        {
            crate::log_info!("[Screen Translate] {error:#}");
            notify_error(&error.to_string());
        }
        crate::overlay::set_is_busy(false);
    });
}

pub(super) fn start_image(path: std::path::PathBuf) {
    if crate::overlay::is_busy() {
        return;
    }
    crate::overlay::set_is_busy(true);
    let (job_id, cancel) = super::runtime::begin_job();
    std::thread::spawn(move || {
        let result = image::open(&path)
            .with_context(|| format!("test image could not be opened: {}", path.display()))
            .map(|image| image.to_rgba8())
            .and_then(|image| {
                let region = CapturedRegion {
                    width: image.width(),
                    height: image.height(),
                    image,
                    left: 420,
                    top: 160,
                };
                translate_region(job_id, Arc::clone(&cancel), region)
            });
        if let Err(error) = result
            && super::runtime::is_current(job_id)
            && !cancel.load(Ordering::SeqCst)
        {
            crate::log_info!("[Screen Translate] {error:#}");
            notify_error(&error.to_string());
        }
        crate::overlay::set_is_busy(false);
    });
}

pub(super) fn process_captured_region(image: image::RgbaImage, rect: RECT) {
    let region = CapturedRegion {
        width: image.width(),
        height: image.height(),
        image,
        left: rect.left,
        top: rect.top,
    };
    let (job_id, cancel) = super::runtime::begin_job();
    std::thread::spawn(move || {
        if let Err(error) = translate_region(job_id, Arc::clone(&cancel), region)
            && super::runtime::is_current(job_id)
            && !cancel.load(Ordering::SeqCst)
        {
            crate::log_info!("[Screen Translate] {error:#}");
            notify_error(&error.to_string());
        }
    });
}

fn translate_region(job_id: u64, cancel: Arc<AtomicBool>, region: CapturedRegion) -> Result<()> {
    let trace_id = format!("screen-translate-{job_id}");
    crate::overlay::result::latency::begin(&trace_id);
    let region_width = i32::try_from(region.width).context("selected region is too wide")?;
    let region_height = i32::try_from(region.height).context("selected region is too tall")?;
    let (target_language, translation_model, translation_prompt, ui_language, graphics_mode) =
        crate::APP
            .lock()
            .map(|app| {
                (
                    app.config.screen_translate.target_language.clone(),
                    app.config.screen_translate.translation_model.clone(),
                    app.config.screen_translate.translation_prompt.clone(),
                    app.config.ui_language.clone(),
                    app.config.graphics_mode.clone(),
                )
            })
            .unwrap_or_else(|_| {
                (
                    "Vietnamese".to_string(),
                    crate::model_config::DEFAULT_TEXT_MODEL_ID.to_string(),
                    crate::config::types::ScreenTranslateSettings::default_prompt(),
                    "en".to_string(),
                    "standard".to_string(),
                )
            });
    let text = crate::gui::locale::LocaleText::get(&ui_language);
    let processing = crate::overlay::process::ProcessingIndicator::show(
        RECT {
            left: region.left,
            top: region.top,
            right: region.left.saturating_add(region_width),
            bottom: region.top.saturating_add(region_height),
        },
        graphics_mode,
    )?;
    crate::overlay::result::latency::mark(&trace_id, "indicator_visible");
    let jpeg = encode_jpeg(&region.image)?;
    crate::overlay::result::latency::mark(&trace_id, "capture_encoded");

    let mut evidence = super::diagnostics::RunEvidence::begin(
        &trace_id,
        &region,
        &jpeg,
        &target_language,
        &translation_model,
        &translation_prompt,
    );

    let detected = match super::detector::detect(
        &jpeg,
        region.image.width(),
        region.image.height(),
        &cancel,
    ) {
        Ok(detected) => detected,
        Err(error) => {
            evidence.fail("detector", &error);
            return Err(error);
        }
    };
    let mut accepted = detected.accepted;
    super::appearance::annotate_regions(&region.image, &mut accepted);
    let candidates = std::sync::Arc::<[super::contract::DetectedTextRegion]>::from(accepted);
    crate::overlay::result::latency::mark(&trace_id, "detector_complete");
    evidence.detected(&candidates, &detected.raw);
    if candidates.is_empty() {
        evidence.no_text();
        crate::overlay::auto_copy_badge::show_notification(
            text.screen_translate.screen_translate_no_text,
        );
        return Ok(());
    }

    let (mut overlay, first_visible) = match super::render::start(
        job_id,
        region,
        std::sync::Arc::clone(&candidates),
        &trace_id,
    ) {
        Ok(renderer) => renderer,
        Err(error) => {
            evidence.fail("renderer_start", &error);
            return Err(error);
        }
    };
    crate::overlay::result::latency::mark(&trace_id, "translation_dispatched");
    let paint_trace_id = trace_id.clone();
    std::thread::spawn(move || {
        if first_visible.recv().is_ok()
            && !crate::overlay::result::latency::wait_for_phase(
                &paint_trace_id,
                "first_painted",
                std::time::Duration::from_secs(3),
            )
        {
            crate::log_info!("[Screen Translate] first result paint acknowledgement timed out");
        }
        processing.close();
    });
    let mut provider_started = false;
    let document = match super::inference::translate(
        &trace_id,
        &target_language,
        &translation_model,
        &translation_prompt,
        &candidates,
        Arc::clone(&cancel),
        |region| {
            if !provider_started {
                provider_started = true;
                crate::overlay::result::latency::mark(&trace_id, "provider_first_output");
            }
            overlay.send(region);
        },
    ) {
        Ok(document) => document,
        Err(error) => {
            evidence.fail("translation", &error);
            return Err(error);
        }
    };
    crate::overlay::result::latency::mark(&trace_id, "translation_complete");
    crate::overlay::result::latency::mark(&trace_id, "provider_complete");
    if super::runtime::is_current(job_id) && !cancel.load(Ordering::SeqCst) {
        if document.regions.is_empty() {
            evidence.no_text();
            crate::overlay::auto_copy_badge::show_notification(
                text.screen_translate.screen_translate_no_text,
            );
            return Ok(());
        }
        let evidence_document = document.clone();
        let region_count = match overlay.complete(document) {
            Ok(region_count) => region_count,
            Err(error) => {
                evidence.fail("renderer_complete", &error);
                return Err(error);
            }
        };
        evidence.finish(evidence_document, region_count);
        if region_count == 0 {
            crate::overlay::auto_copy_badge::show_notification(
                text.screen_translate.screen_translate_no_text,
            );
        }
        crate::log_info!("[Screen Translate] ready regions={region_count}");
    }
    Ok(())
}

fn notify_error(error: &str) {
    let language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    let text = crate::gui::locale::LocaleText::get(&language);
    crate::overlay::auto_copy_badge::show_detailed_notification(
        text.screen_translate.screen_translate_error,
        error,
        crate::overlay::auto_copy_badge::NotificationType::Error,
    );
}

fn capture_foreground() -> Result<CapturedRegion> {
    let capture = crate::screen_capture::capture_screen_fast().context("screen capture failed")?;
    let virtual_left = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let virtual_top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let virtual_width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let virtual_height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
    let mut window_rect = RECT::default();
    let window = unsafe { GetForegroundWindow() };
    unsafe { GetWindowRect(window, &mut window_rect) }
        .context("foreground window bounds failed")?;
    let left = (window_rect.left - virtual_left).clamp(0, virtual_width.saturating_sub(1));
    let top = (window_rect.top - virtual_top).clamp(0, virtual_height.saturating_sub(1));
    let right = (window_rect.right - virtual_left).clamp(left + 1, virtual_width);
    let bottom = (window_rect.bottom - virtual_top).clamp(top + 1, virtual_height);
    let crop_rect = RECT {
        left,
        top,
        right,
        bottom,
    };
    let image = crate::overlay::selection::extract_crop_from_hbitmap_public(&capture, crop_rect);
    Ok(CapturedRegion {
        image,
        left: left + virtual_left,
        top: top + virtual_top,
        width: (right - left) as u32,
        height: (bottom - top) as u32,
    })
}

pub(crate) fn encode_jpeg(image: &image::RgbaImage) -> Result<Vec<u8>> {
    let rgb = image::DynamicImage::ImageRgba8(image.clone()).to_rgb8();
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, 88).encode(
        rgb.as_raw(),
        rgb.width(),
        rgb.height(),
        ExtendedColorType::Rgb8,
    )?;
    Ok(jpeg)
}
