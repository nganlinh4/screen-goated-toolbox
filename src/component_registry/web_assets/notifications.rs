use super::WebAssetComponent;

pub(super) fn set_download_state(component: WebAssetComponent, progress: f32) {
    let (title, message) = progress_text(component);
    if let Ok(mut state) = crate::overlay::realtime_webview::state::REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = title.clone();
        state.download_message = message.clone();
        state.download_progress = progress;
    }
}

pub(super) fn download_title(component: WebAssetComponent) -> String {
    progress_text(component).0
}

pub(super) fn finish_download_state(success: bool) {
    if let Ok(mut state) = crate::overlay::realtime_webview::state::REALTIME_STATE.lock() {
        state.is_downloading = false;
        state.download_progress = if success { 100.0 } else { 0.0 };
    }
}

pub(super) fn notify_success(component: WebAssetComponent) {
    let name = localized_name(component);
    let locale = crate::overlay::auto_copy_badge::locale_text();
    let installed = crate::overlay::auto_copy_badge::format_locale(
        locale.component_installed_fmt,
        &[("name", &name)],
    );
    crate::overlay::auto_copy_badge::show_detailed_notification(
        &installed,
        &name,
        crate::overlay::auto_copy_badge::NotificationType::Success,
    );
}

pub(super) fn localized_name(component: WebAssetComponent) -> String {
    let language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    let managed = crate::gui::locale::LocaleText::get(&language)
        .auxiliary
        .managed_tools;
    match component {
        WebAssetComponent::Creation3d => managed.tool_creation_interface,
        WebAssetComponent::PromptDj => managed.tool_prompt_dj_interface,
        WebAssetComponent::TtsPlayground => managed.tool_tts_playground_interface,
    }
    .to_string()
}

fn progress_text(component: WebAssetComponent) -> (String, String) {
    let name = localized_name(component);
    let locale = crate::overlay::auto_copy_badge::locale_text();
    let title = crate::overlay::auto_copy_badge::format_locale(
        locale.downloading_component_fmt,
        &[("name", &name)],
    );
    (title, name)
}
