use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use anyhow::{Context, Result};
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetSystemMetrics, GetWindowRect, SM_CXSCREEN, SM_CXVIRTUALSCREEN,
    SM_CYSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

use crate::overlay::selection::CapturedRegion;

pub(super) fn start_foreground() {
    if !crate::overlay::try_claim_overlay() {
        return;
    }
    let (job_id, cancel) = super::runtime::begin_job();
    std::thread::spawn(move || {
        let result = capture_foreground().and_then(|region| {
            super::incremental::translate_region(job_id, Arc::clone(&cancel), region)
        });
        report_job_end(job_id, &cancel, result);
        crate::overlay::set_is_busy(false);
    });
}

pub(super) fn start_image(path: std::path::PathBuf) {
    if !crate::overlay::try_claim_overlay() {
        return;
    }
    let (job_id, cancel) = super::runtime::begin_job();
    std::thread::spawn(move || {
        let result = image::open(&path)
            .with_context(|| format!("test image could not be opened: {}", path.display()))
            .map(|image| image.to_rgba8())
            .and_then(|image| {
                let (left, top) = image_origin(image.width(), image.height());
                let region = CapturedRegion {
                    width: image.width(),
                    height: image.height(),
                    image,
                    left,
                    top,
                };
                super::incremental::translate_region(job_id, Arc::clone(&cancel), region)
            });
        report_job_end(job_id, &cancel, result);
        crate::overlay::set_is_busy(false);
    });
}

#[cfg(debug_assertions)]
pub(super) fn start_lab_image(path: std::path::PathBuf, admission: super::LabSnapshot) {
    std::thread::spawn(move || {
        let Ok(image) = image::open(&path).map(|image| image.to_rgba8()) else {
            return;
        };
        let Some((job_id, cancel, _)) = super::admit_lab_job(admission) else {
            return;
        };
        super::prepare_detector();
        let (left, top) = image_origin(image.width(), image.height());
        let region = CapturedRegion {
            width: image.width(),
            height: image.height(),
            image,
            left,
            top,
        };
        let result = super::incremental::translate_region(job_id, Arc::clone(&cancel), region);
        report_job_end(job_id, &cancel, result);
        crate::overlay::set_is_busy(false);
    });
}

pub(super) fn image_origin(width: u32, height: u32) -> (i32, i32) {
    // The lab source window uses the same physical-pixel placement, including
    // cached-layout replay. Keep tall inputs inside the visible desktop.
    let left = unsafe { GetSystemMetrics(SM_CXSCREEN) }
        .saturating_sub(width as i32)
        .saturating_sub(16)
        .clamp(0, 420);
    let top = unsafe { GetSystemMetrics(SM_CYSCREEN) }
        .saturating_sub(height as i32)
        .saturating_sub(16)
        .clamp(0, 160);
    (left, top)
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
        let result = super::incremental::translate_region(job_id, Arc::clone(&cancel), region);
        report_job_end(job_id, &cancel, result);
    });
}

pub(super) fn notify_error(error: &str) {
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

/// A user cancel or superseded job stays silent; every failure is shown.
fn report_job_end(job_id: u64, cancel: &AtomicBool, result: Result<()>) {
    match super::runtime::end_job(job_id, cancel, result) {
        super::runtime::JobEnd::Failed(error) => {
            crate::log_info!("[Screen Translate] {error:#}");
            notify_job_error(job_id, &error.to_string());
        }
        // The component already ended the job, so the notice is not gated on it.
        super::runtime::JobEnd::ComponentFailed(cause) => notify_error(&cause),
        super::runtime::JobEnd::Completed | super::runtime::JobEnd::Cancelled => {}
    }
}

fn notify_job_error(job_id: u64, error: &str) {
    let language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    let text = crate::gui::locale::LocaleText::get(&language);
    let _ = super::runtime::with_current_primary(job_id, || {
        crate::overlay::auto_copy_badge::show_detailed_notification(
            text.screen_translate.screen_translate_error,
            error,
            crate::overlay::auto_copy_badge::NotificationType::Error,
        );
    });
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
