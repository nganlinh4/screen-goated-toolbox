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

pub(super) fn start() {
    if crate::overlay::is_busy() || crate::overlay::is_selection_overlay_active() {
        return;
    }
    crate::overlay::set_is_busy(true);
    let (job_id, cancel) = super::runtime::begin_job();
    super::detector::prepare(Arc::clone(&cancel));
    std::thread::spawn(move || {
        if let Err(error) = capture_and_translate(job_id, Arc::clone(&cancel))
            && super::runtime::is_current(job_id)
            && !cancel.load(Ordering::SeqCst)
        {
            crate::log_info!("[Screen Translate] {error:#}");
            notify_error(&error.to_string());
        }
        crate::overlay::set_is_busy(false);
    });
}

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

fn capture_and_translate(job_id: u64, cancel: Arc<AtomicBool>) -> Result<()> {
    let capture = crate::screen_capture::capture_screen_fast().context("screen capture failed")?;
    {
        let mut app = crate::APP
            .lock()
            .map_err(|_| anyhow::anyhow!("app state is unavailable"))?;
        app.screenshot_handle = Some(capture);
    }

    let (sender, receiver) = std::sync::mpsc::channel();
    crate::overlay::show_capture_overlay(sender);
    let Some(region) = receive_selection(receiver) else {
        return Ok(());
    };
    crate::overlay::set_is_busy(false);
    if !super::runtime::is_current(job_id) || cancel.load(Ordering::SeqCst) {
        return Ok(());
    }

    translate_region(job_id, cancel, region)
}

fn receive_selection(
    receiver: std::sync::mpsc::Receiver<CapturedRegion>,
) -> Option<CapturedRegion> {
    receiver.recv().ok()
}

fn translate_region(job_id: u64, cancel: Arc<AtomicBool>, region: CapturedRegion) -> Result<()> {
    let trace_id = format!("screen-translate-{job_id}");
    crate::overlay::result::latency::begin(&trace_id);
    let region_width = i32::try_from(region.width).context("selected region is too wide")?;
    let region_height = i32::try_from(region.height).context("selected region is too tall")?;
    let (target_language, ui_language, graphics_mode) = crate::APP
        .lock()
        .map(|app| {
            (
                app.config.screen_translate.target_language.clone(),
                app.config.ui_language.clone(),
                app.config.graphics_mode.clone(),
            )
        })
        .unwrap_or_else(|_| {
            (
                "Vietnamese".to_string(),
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

    let candidates =
        super::detector::detect(&jpeg, region.image.width(), region.image.height(), &cancel)?;
    crate::overlay::result::latency::mark(&trace_id, "detector_complete");
    if candidates.is_empty() {
        crate::overlay::auto_copy_badge::show_notification(
            text.screen_translate.screen_translate_no_text,
        );
        return Ok(());
    }

    crate::overlay::result::latency::mark(&trace_id, "translation_dispatched");
    let (mut overlay, first_visible) =
        super::render::start(job_id, &region, &candidates, &trace_id)?;
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
    let document = super::inference::translate(
        &target_language,
        &candidates,
        Arc::clone(&cancel),
        |event| {
            if !provider_started && matches!(event, super::inference::TranslationEvent::Region(_)) {
                provider_started = true;
                crate::overlay::result::latency::mark(&trace_id, "provider_first_output");
            }
            overlay.send(event);
        },
    )?;
    crate::overlay::result::latency::mark(&trace_id, "translation_complete");
    crate::overlay::result::latency::mark(&trace_id, "provider_complete");
    if super::runtime::is_current(job_id) && !cancel.load(Ordering::SeqCst) {
        if document.regions.is_empty() {
            crate::overlay::auto_copy_badge::show_notification(
                text.screen_translate.screen_translate_no_text,
            );
            return Ok(());
        }
        let region_count = overlay.complete(document)?;
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

#[cfg(test)]
mod tests {
    use super::receive_selection;

    #[test]
    fn a_closed_selection_channel_is_a_normal_cancellation() {
        let (sender, receiver) = std::sync::mpsc::channel();
        drop(sender);
        assert!(receive_selection(receiver).is_none());
    }
}
