//! Fast in-place translation mini app built on the shared screen-selection flow.

mod backdrop;
mod capture;
pub(crate) mod contract;
mod detector;
pub(crate) mod geometry;
mod inference;
mod render;
mod runtime;
mod stream_parser;

static UI_TEST_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn translate_screen() {
    capture::start();
}

pub(crate) fn prepare_detector() {
    detector::prepare(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
        false,
    )));
}

pub(crate) fn run_ui_test(image_path: Option<std::path::PathBuf>) {
    UI_TEST_MODE.store(true, std::sync::atomic::Ordering::SeqCst);
    if let Some(image_path) = image_path {
        capture::start_image(image_path);
        return;
    }
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(1_500));
        capture::start_foreground();
    });
}

fn is_ui_test() -> bool {
    UI_TEST_MODE.load(std::sync::atomic::Ordering::SeqCst)
}

pub(crate) fn stop_detector() {
    detector::stop();
}
