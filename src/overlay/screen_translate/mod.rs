//! Fast in-place translation mini app built on the shared screen-selection flow.

mod appearance;
mod backdrop;
mod capture;
pub(crate) mod contract;
mod detector;
mod diagnostics;
#[cfg(debug_assertions)]
mod evidence_capture;
pub(crate) mod geometry;
mod inference;
mod layout;
mod render;
mod runtime;
mod schema;
pub(crate) mod stream_parser;

static CAPTURE_HANDLER: crate::overlay::image_capture_target::ImageCaptureHandler =
    crate::overlay::image_capture_target::ImageCaptureHandler {
        prepare: prepare_detector,
        process: process_captured_region,
        localized_name: capture_label,
    };

pub(crate) fn capture_target() -> crate::overlay::image_capture_target::ImageCaptureTarget {
    crate::overlay::image_capture_target::ImageCaptureTarget::Handler(&CAPTURE_HANDLER)
}

pub(crate) fn prepare_detector() {
    detector::prepare(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
        false,
    )));
}

pub(crate) fn prepare_installed_detector() {
    if !crate::component_registry::screen_text_detector::is_installed()
        || !matches!(
            crate::component_registry::local_asr::current_status(
                crate::component_registry::local_asr::ComponentKind::Runtime
            ),
            crate::component_registry::local_asr::ComponentStatus::Installed { .. }
        )
        || !matches!(
            crate::component_registry::vc_runtime::current_status(),
            crate::component_registry::vc_runtime::VcRuntimeStatus::Installed { .. }
        )
    {
        return;
    }
    prepare_detector();
}

pub(crate) fn process_captured_region(
    image: image::RgbaImage,
    rect: windows::Win32::Foundation::RECT,
) {
    capture::process_captured_region(image, rect);
}

fn capture_label(ui_language: &str) -> String {
    crate::gui::locale::LocaleText::get(ui_language)
        .screen_translate
        .screen_translate_title
        .to_string()
}

pub(crate) fn run_ui_test(image_path: Option<std::path::PathBuf>) {
    if let Some(image_path) = image_path {
        capture::start_image(image_path);
        return;
    }
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(1_500));
        capture::start_foreground();
    });
}

pub(crate) fn stop_detector() {
    detector::stop();
}
