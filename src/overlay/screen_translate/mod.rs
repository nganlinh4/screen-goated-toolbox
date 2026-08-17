//! Fast in-place translation mini app built on the shared screen-selection flow.

mod appearance;
mod backdrop;
mod capture;
mod cell_proposals;
mod cell_validation;
pub(crate) mod contract;
mod detector;
#[cfg(debug_assertions)]
mod diagnostic_raw;
mod diagnostics;
#[cfg(debug_assertions)]
mod evidence_capture;
pub(crate) mod geometry;
mod inference;
mod render;
mod render_scene;
#[cfg(debug_assertions)]
mod replay;
mod runtime;
mod schema;
pub(crate) mod stream_parser;
mod text_metrics;
mod vision_fallback;

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

#[cfg(debug_assertions)]
pub(crate) fn run_lab_queue(queue: std::path::PathBuf) {
    prepare_detector();
    std::thread::Builder::new()
        .name("sgt-screen-translate-lab-queue".to_string())
        .spawn(move || {
            let request = queue.join("request.json");
            loop {
                if request.is_file() && !crate::overlay::is_busy() {
                    let value = std::fs::read(&request)
                        .ok()
                        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
                    let _ = std::fs::remove_file(&request);
                    if let Some(value) = value {
                        if value.get("action").and_then(|item| item.as_str()) == Some("replay") {
                            replay::start(value);
                        } else if let Some(image) = value
                            .get("image")
                            .and_then(|item| item.as_str())
                            .map(std::path::PathBuf::from)
                            .filter(|path| path.is_absolute() && path.is_file())
                        {
                            capture::start_image(image);
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        })
        .ok();
}

#[cfg(not(debug_assertions))]
pub(crate) fn run_lab_queue(_queue: std::path::PathBuf) {}

pub(crate) fn stop_detector() {
    detector::stop();
}
