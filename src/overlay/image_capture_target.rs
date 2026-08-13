//! Processing target shared by normal and continuous image-region capture.

use windows::Win32::Foundation::RECT;

pub struct ImageCaptureHandler {
    pub prepare: fn(),
    pub process: fn(image::RgbaImage, RECT),
    pub localized_name: fn(&str) -> String,
}

#[derive(Clone, Copy)]
pub enum ImageCaptureTarget {
    Preset(usize),
    Handler(&'static ImageCaptureHandler),
}

impl std::fmt::Debug for ImageCaptureTarget {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Preset(index) => formatter.debug_tuple("Preset").field(index).finish(),
            Self::Handler(_) => formatter.write_str("Handler"),
        }
    }
}

impl PartialEq for ImageCaptureTarget {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Preset(left), Self::Preset(right)) => left == right,
            (Self::Handler(left), Self::Handler(right)) => std::ptr::eq(*left, *right),
            _ => false,
        }
    }
}

impl Eq for ImageCaptureTarget {}

impl Default for ImageCaptureTarget {
    fn default() -> Self {
        Self::Preset(0)
    }
}

impl ImageCaptureTarget {
    pub fn prepare(self) {
        if let Self::Handler(handler) = self {
            (handler.prepare)();
        }
    }

    pub fn is_master(self) -> bool {
        let Self::Preset(index) = self else {
            return false;
        };
        crate::APP
            .lock()
            .ok()
            .and_then(|app| app.config.presets.get(index).map(|preset| preset.is_master))
            .unwrap_or(false)
    }

    pub fn process(self, image: image::RgbaImage, rect: RECT) {
        match self {
            Self::Preset(index) => {
                let Some((config, preset)) = crate::APP.lock().ok().and_then(|mut app| {
                    app.config.active_preset_idx = index;
                    let preset = app.config.presets.get(index)?.clone();
                    Some((app.config.clone(), preset))
                }) else {
                    return;
                };
                std::thread::spawn(move || {
                    crate::overlay::process::start_processing_pipeline(image, rect, config, preset);
                });
            }
            Self::Handler(handler) => (handler.process)(image, rect),
        }
    }

    pub fn localized_name(self, ui_language: &str) -> String {
        match self {
            Self::Preset(index) => crate::APP
                .lock()
                .ok()
                .and_then(|app| {
                    app.config
                        .presets
                        .get(index)
                        .map(|preset| preset.id.clone())
                })
                .map(|id| crate::gui::settings_ui::get_localized_preset_name(&id, ui_language))
                .unwrap_or_default(),
            Self::Handler(handler) => (handler.localized_name)(ui_language),
        }
    }
}
