use crate::gui::locale::GlobalSettingsLocaleText;

pub(super) fn get() -> GlobalSettingsLocaleText {
    GlobalSettingsLocaleText {
        controller_checkbox_label: "Controller",
        api_keys_header: "API Keys",
        groq_label: "Groq API Key:",
        software_update_header: "Software Update",
        donate_header: "Support the Developer",
        donate_body: "SGT is free software. If you find it useful, you can support the developer with a Vietnamese bank transfer (VietQR). Thank you!",
        donate_note: "Bank transfer is available to Vietnamese donors only.",
        donate_vietnamese: false,
        startup_display_header: "Startup & Display",
        favorite_overlay_opacity_label: "Favorite overlay opacity",
        model_thinking: "Thinking...",
        ollama_url_guide: "View guide at ollama.com",
    }
}
