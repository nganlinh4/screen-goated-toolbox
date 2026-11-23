use crate::gui::locale::LocaleText;
use crate::overlay::html_components::icons::OverlayIcon;

pub struct RealtimeHtmlOptions<'a> {
    pub is_translation: bool,
    pub audio_source: &'a str,
    pub languages: &'a [String],
    pub current_language: &'a str,
    pub translation_model: &'a str,
    pub transcription_model: &'a str,
    pub font_size: u32,
    pub text: &'a LocaleText,
    pub is_dark: bool,
    /// Identifies a card hosted inside the unified realtime compositor.
    /// Standalone documents leave this unset and post directly to WRY.
    pub compositor_role: Option<&'a str>,
}

pub fn get_realtime_html(options: RealtimeHtmlOptions<'_>) -> String {
    let RealtimeHtmlOptions {
        is_translation,
        audio_source,
        languages,
        current_language,
        translation_model,
        transcription_model,
        font_size,
        text,
        is_dark,
        compositor_role,
    } = options;
    let _title_icon = if is_translation {
        "translate"
    } else {
        "graphic_eq"
    };
    let is_s2s = crate::model_config::is_gemini_live_s2s_model_id(transcription_model);
    let is_live_translate =
        transcription_model == crate::model_config::GEMINI_LIVE_TRANSLATE_MODEL_ID;
    let glow_color = crate::overlay::utils::glow_color(is_translation);

    // Volume canvas lives inside #controls so it scrolls with the header (matches Android)
    let title_content = String::new();

    let _mic_text = text.realtime.microphone;
    let _device_text = text.realtime.device;
    let placeholder_text = text.realtime.waiting;

    // Build language options HTML - show full name in dropdown, but store code for display
    let lang_options: String = languages
        .iter()
        .map(|lang| {
            let selected = if lang == current_language {
                "selected"
            } else {
                ""
            };
            // Get 2-letter ISO 639-1 code
            let lang_code = crate::config::language_from_name(lang)
                .and_then(|l| l.to_639_1())
                .map(|c| c.to_uppercase())
                .unwrap_or_else(|| lang.chars().take(2).collect::<String>().to_uppercase());
            // Option shows full name, but we store code as data attribute for selected display
            format!(
                r#"<option value="{}" data-code="{}" {}>{}</option>"#,
                lang, lang_code, selected, lang
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Audio source selector (only for transcription window) - simple mic/device toggle
    let audio_selector = if !is_translation {
        let is_device = audio_source == "device";

        {
            // Build transcription model dropdown options from the shared model catalog.
            let trans_options = crate::model_config::realtime_transcription_model_options();
            let options_html: String = trans_options
                .iter()
                .map(|(val, label)| {
                    let selected = if *val == transcription_model {
                        " selected"
                    } else {
                        ""
                    };
                    format!(r#"<option value="{val}"{selected}>{label}</option>"#)
                })
                .collect::<Vec<_>>()
                .join("\n");

            // The transcription language dropdown is active only for selectors
            // that transcribe an explicitly chosen source language.
            let requires_source_language =
                crate::model_config::realtime_transcription_requires_source_language(
                    transcription_model,
                );
            let trans_lang_code = {
                let app = crate::APP.lock().unwrap();
                app.config.realtime_transcription_language.clone()
            };
            let is_all_lang = crate::model_config::get_all_models()
                .iter()
                .any(|model| model.id == transcription_model && model.provider == "gemini-live");
            let mut effective_lang = if is_all_lang { "all" } else { &trans_lang_code };
            if requires_source_language && effective_lang == "all" {
                effective_lang = "en";
            }
            let source_lang_options = [
                ("en", "English"),
                ("ko", "Korean"),
                ("zh", "Chinese"),
                ("fr", "French"),
                ("de", "German"),
                ("es", "Spanish"),
                ("ru", "Russian"),
                ("all-8", "AR,EN,ID,JA,RU,TH,VI,ZH"),
            ];
            let any_lang_options = [
                ("all", "All"),
                ("en", "English"),
                ("ko", "Korean"),
                ("zh", "Chinese"),
                ("fr", "French"),
                ("de", "German"),
                ("es", "Spanish"),
                ("ru", "Russian"),
                ("all-8", "AR,EN,ID,JA,RU,TH,VI,ZH"),
            ];
            let trans_lang_options: &[(&str, &str)] = if requires_source_language {
                &source_lang_options
            } else {
                &any_lang_options
            };
            let trans_lang_html: String = trans_lang_options
                .iter()
                .map(|(code, name)| {
                    let selected = if *code == effective_lang {
                        " selected"
                    } else {
                        ""
                    };
                    format!(r#"<option value="{code}"{selected}>{name}</option>"#)
                })
                .collect::<Vec<_>>()
                .join("\n");
            let trans_lang_disabled = if requires_source_language {
                ""
            } else {
                "disabled"
            };
            let trans_lang_hidden = if requires_source_language {
                ""
            } else {
                "hidden"
            };

            format!(
                r#"
                <canvas id="volume-canvas" width="90" height="24"></canvas>
                <div class="btn-group">
                    <span class="inline-svg-icon audio-icon {mic_active}" id="mic-btn" data-value="mic" title="{mic_title}">{mic_svg}</span>
                    <span class="inline-svg-icon audio-icon {device_active}" id="device-btn" data-value="device" title="{device_title}">{device_svg}</span>
                </div>
                <select class="model-dropdown" id="transcription-model-select" title="{model_title}">
                    {options_html}
                </select>
                <span class="ctrl-btn" id="custom-vocabulary-btn" title="{vocabulary_title}" {vocabulary_hidden}><span class="inline-svg-icon">{vocabulary_svg}</span></span>
                <select class="model-dropdown" id="transcription-lang-select" title="{language_title}" {trans_lang_disabled} {trans_lang_hidden}>
                    {trans_lang_html}
                </select>
            "#,
                mic_active = if !is_device { "active" } else { "" },
                device_active = if is_device { "active" } else { "" },
                mic_svg = crate::overlay::html_components::icons::OverlayIcon::Mic.svg(),
                device_svg =
                    crate::overlay::html_components::icons::OverlayIcon::SpeakerGroup.svg(),
                mic_title = text.realtime.realtime_tooltip_microphone_input,
                device_title = text.realtime.realtime_tooltip_device_audio,
                model_title = text.realtime.realtime_tooltip_transcription_model,
                vocabulary_title = text.realtime.realtime_tooltip_custom_vocabulary,
                vocabulary_hidden = if crate::model_config::realtime_transcription_live_protocol(
                    transcription_model
                ) == Some("live-transcribe")
                {
                    ""
                } else {
                    "hidden"
                },
                vocabulary_svg =
                    crate::overlay::html_components::icons::OverlayIcon::Dictionary.svg(),
                language_title = text.realtime.realtime_tooltip_transcription_language,
                options_html = options_html,
                trans_lang_html = trans_lang_html,
                trans_lang_disabled = trans_lang_disabled,
                trans_lang_hidden = trans_lang_hidden,
            )
        }
    } else {
        // Language selector and model dropdown for translation window
        {
            let llm_id = crate::model_config::REALTIME_TRANSLATION_MODEL_LLM;
            let gtx_id = crate::model_config::REALTIME_TRANSLATION_MODEL_GTX;

            let trans_model_options = [
                (llm_id, text.shell.llm_label),
                (gtx_id, text.shell.google_gtx_label),
            ];
            let model_options_html: String = trans_model_options
                .iter()
                .map(|(val, label)| {
                    let selected = if *val == translation_model {
                        " selected"
                    } else {
                        ""
                    };
                    format!(r#"<option value="{val}"{selected}>{label}</option>"#)
                })
                .collect::<Vec<_>>()
                .join("\n");

            format!(
                r#"
                <span class="ctrl-btn speak-btn {speak_active}" id="speak-btn" title="{speak_title}"><span class="inline-svg-icon">{volume_up_svg}</span></span>
                <select class="model-dropdown" id="translation-model-select" title="{translation_model_title}" {translation_model_disabled} {translation_model_hidden}>
                    {model_options_html}
                </select>
                <select id="language-select" title="{language_title}" {language_disabled}>
                    {lang_options}
                </select>
            "#,
                lang_options = lang_options,
                model_options_html = model_options_html,
                translation_model_title = if is_s2s {
                    text.realtime.realtime_tooltip_s2s_translation_model
                } else {
                    text.realtime.realtime_tooltip_translation_model
                },
                translation_model_disabled = if is_s2s { "disabled" } else { "" },
                translation_model_hidden = if is_s2s { "hidden" } else { "" },
                speak_active = if is_s2s { "active locked" } else { "" },
                speak_title = if is_s2s {
                    text.realtime.realtime_tooltip_direct_speech
                } else {
                    text.realtime.realtime_tooltip_tts_settings
                },
                language_title = if is_s2s {
                    text.realtime.realtime_tooltip_s2s_target_language
                } else {
                    text.realtime.realtime_tooltip_target_language
                },
                language_disabled = "",
                volume_up_svg = crate::overlay::html_components::icons::OverlayIcon::VolumeUp.svg(),
            )
        }
    };

    let (loading_symbol, loading_color) = if is_translation {
        (OverlayIcon::Translate, "#ff9633")
    } else {
        (OverlayIcon::GraphicEq, "#00c8ff")
    };
    let loading_icon = loading_symbol.svg().replacen(
        "<svg ",
        &format!(r#"<svg class="loading-svg" style="color: {loading_color}" "#),
        1,
    );

    // Construct CSS and JS from components
    let css = format!(
        "{}{}",
        crate::overlay::html_components::css_main::get(glow_color, font_size, is_dark),
        crate::overlay::html_components::css_modals::get(is_dark)
    );
    let js = format!(
        "{}{}",
        crate::overlay::html_components::js_main::get(font_size),
        crate::overlay::html_components::js_logic::get(placeholder_text, is_translation)
    );
    let l10n_json = serde_json::json!({
        "s2sModelIds": crate::model_config::GENERATED_REALTIME_S2S_MODEL_IDS,
        "sourceLanguageModelIds": crate::model_config::GENERATED_REALTIME_SOURCE_LANGUAGE_MODEL_IDS,
        "translationModel": text.realtime.realtime_tooltip_translation_model,
        "s2sTranslationModel": text.realtime.realtime_tooltip_s2s_translation_model,
        "targetLanguage": text.realtime.realtime_tooltip_target_language,
        "s2sTargetLanguage": text.realtime.realtime_tooltip_s2s_target_language,
        "directSpeech": text.realtime.realtime_tooltip_direct_speech,
        "ttsSettings": text.realtime.realtime_tooltip_tts_settings,
        "ttsS2sLocked": text.realtime.realtime_tts_s2s_locked_tooltip,
        "ttsEnable": text.realtime.realtime_tts_enable_tooltip,
    })
    .to_string();
    let l10n_json = crate::overlay::utils::inline_script_json(&l10n_json);

    // Get local font CSS (cached fonts, no network loading)
    let font_css = crate::overlay::html_components::font_manager::get_font_css();
    let ipc_bootstrap = if let Some(role) = compositor_role {
        let role = serde_json::to_string(role).unwrap_or_else(|_| "\"transcription\"".to_string());
        include_str!("realtime_webview/card_session.js").replace("__SGT_REALTIME_ROLE__", &role)
    } else {
        "window.REALTIME_COMPOSITOR_ROLE=null;window.realtimePostMessage=function(message){window.ipc.postMessage(message);};".to_string()
    };

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <style>{font_css}</style>
    <style id="main-style">
        {css_content}
    </style>
</head>
<body data-s2s="{is_s2s_attr}" data-live-translate="{is_live_translate_attr}">
    <div id="loading-overlay">{loading_icon}</div>
    <div id="container">
        <div id="header">
            <div id="title">{title_content}</div>
            <div id="controls">
                {audio_selector}
                <span class="ctrl-btn" id="copy-btn" title="{copy_title}"><span class="inline-svg-icon">{content_copy_svg}</span></span>
                <div class="pill-group">
                    <span class="ctrl-btn" id="font-decrease" title="{font_decrease_title}"><span class="inline-svg-icon">{remove_svg}</span></span>
                    <span class="ctrl-btn" id="font-increase" title="{font_increase_title}"><span class="inline-svg-icon">{add_svg}</span></span>
                </div>
                <div class="btn-group">
                    <span class="vis-btn mic active" id="toggle-mic" title="{toggle_mic_title}"><span class="inline-svg-icon">{subtitles_svg}</span></span>
                    <span class="vis-btn trans active" id="toggle-trans" title="{toggle_trans_title}"><span class="inline-svg-icon">{translate_svg}</span></span>
                </div>
            </div>
        </div>
        <div id="header-toggle" title="{toggle_header_title}"><span class="inline-svg-icon">{expand_less_svg}</span></div>
        <div id="viewport">
            <div id="content">
                <span class="placeholder">{placeholder_text}</span>
            </div>
        </div>
        <div id="resize-hint"><span class="inline-svg-icon" style="font-size: 20px;">{pip_svg}</span></div>
    </div>
    <!-- Custom Vocabulary Modal -->
    <div id="custom-vocabulary-modal-overlay"></div>
    <div id="custom-vocabulary-modal" role="dialog" aria-modal="true" aria-labelledby="custom-vocabulary-modal-title">
        <div class="custom-vocabulary-modal-title">
            <span class="inline-svg-icon">{dictionary_svg}</span>
            <span id="custom-vocabulary-modal-title">{vocabulary_title}</span>
        </div>
        <div class="custom-vocabulary-entry" title="{vocabulary_prompt}">
            <div id="custom-vocabulary-pills" role="group"></div>
            <input id="custom-vocabulary-input" type="text" spellcheck="false" autocomplete="off" aria-label="{vocabulary_prompt}" placeholder="{vocabulary_prompt}">
            <button type="button" id="custom-vocabulary-add" title="{vocabulary_title}" aria-label="{vocabulary_title}"><span class="inline-svg-icon">{add_svg}</span></button>
        </div>
        <template id="custom-vocabulary-pill-close">{close_svg}</template>
    </div>
    <!-- Download Modal -->
    <div id="download-modal-overlay"></div>
    <div id="download-modal">
        <div class="download-modal-title">
            <span class="inline-svg-icon">{download_svg}</span>
            <span id="download-title">{download_default_title}</span>
        </div>
        <div class="download-modal-msg" id="download-msg">{download_wait}</div>
        <div class="download-progress-bar">
            <div class="download-progress-fill" id="download-fill" style="width: 0%;"></div>
        </div>
        <button class="download-cancel-btn" id="download-cancel-btn" title="{download_cancel_title}">
            <span class="inline-svg-icon">{close_svg}</span>
            {cancel_text}
        </button>
    </div>
    <!-- TTS Settings Modal -->
    <div id="tts-modal-overlay"></div>
    <div id="tts-modal">
        <div class="tts-modal-title">
            <span class="inline-svg-icon">{volume_up_svg}</span>
            {tts_title}
                <div class="toggle-switch tts-toggle-control {tts_toggle_class}" id="tts-toggle" title="{tts_toggle_title}" style="margin-left: auto;"></div>
        </div>
        <div class="tts-modal-row tts-speed-row">
            <span class="tts-modal-label">{tts_speed}</span>
            <div class="speed-slider-container">
                <input type="range" class="speed-slider" id="speed-slider" min="50" max="200" value="100" step="10">
                <span class="speed-value" id="speed-value">1.0x</span>
                <button class="auto-toggle on" id="auto-speed-toggle" title="{tts_auto_title}">{tts_auto}</button>
            </div>
        </div>
        <div class="tts-modal-row tts-volume-row">
            <span class="tts-modal-label">{tts_volume}</span>
            <div class="speed-slider-container">
                <input type="range" class="speed-slider" id="volume-slider" min="0" max="100" value="100" step="5">
                <span class="speed-value" id="volume-value">100%</span>
            </div>
        </div>
    </div>
    <!-- App Selection Modal -->
    <div id="app-modal-overlay"></div>
    <div id="app-modal">
        <div class="app-modal-title">
            <span class="inline-svg-icon">{apps_svg}</span>
            {app_select_title}
        </div>
        <div class="app-modal-hint">{app_select_hint}</div>
        <div id="app-list" class="app-list">
            <div class="app-loading">{app_loading}</div>
        </div>
    </div>
    <script>
        {ipc_bootstrap}
        window.REALTIME_L10N = {l10n_json};
        window.REALTIME_CUSTOM_VOCABULARY = {vocabulary_json};
        {js_content}
    </script>
</body>
</html>"#,
        css_content = css,
        js_content = js,
        l10n_json = l10n_json,
        vocabulary_json =
            vocabulary_script_json(&crate::api::realtime_audio::vocabulary_snapshot().1),
        ipc_bootstrap = ipc_bootstrap,
        is_s2s_attr = if is_s2s { "1" } else { "0" },
        is_live_translate_attr = if is_live_translate { "1" } else { "0" },
        tts_toggle_class = if is_s2s { "on locked" } else { "" },
        tts_toggle_title = if is_s2s {
            text.realtime.realtime_tts_s2s_locked_tooltip
        } else {
            text.realtime.realtime_tts_enable_tooltip
        },
        loading_icon = loading_icon,
        title_content = title_content,
        audio_selector = audio_selector,
        placeholder_text = placeholder_text,
        copy_title = text.realtime.realtime_tooltip_copy_text,
        font_decrease_title = text.realtime.realtime_tooltip_decrease_font,
        font_increase_title = text.realtime.realtime_tooltip_increase_font,
        toggle_mic_title = text.shell.toggle_transcription_tooltip,
        toggle_trans_title = text.shell.toggle_translation_tooltip,
        toggle_header_title = text.realtime.realtime_tooltip_toggle_header,
        download_default_title = text.realtime.realtime_download_default_title,
        download_wait = text.realtime.realtime_download_wait,
        download_cancel_title = text.realtime.realtime_download_cancel_tooltip,
        tts_auto_title = text.realtime.realtime_tts_auto_tooltip,
        app_loading = text.realtime.realtime_app_loading,
        tts_title = text.realtime.realtime_tts_title,
        tts_speed = text.realtime.realtime_tts_speed,
        tts_auto = text.realtime.realtime_tts_auto,
        tts_volume = text.realtime.realtime_tts_volume,
        app_select_title = text.realtime.app_select_title,
        app_select_hint = text.realtime.app_select_hint,
        content_copy_svg = crate::overlay::html_components::icons::OverlayIcon::ContentCopy.svg(),
        remove_svg = crate::overlay::html_components::icons::OverlayIcon::Remove.svg(),
        add_svg = crate::overlay::html_components::icons::OverlayIcon::Add.svg(),
        subtitles_svg = crate::overlay::html_components::icons::OverlayIcon::Subtitles.svg(),
        translate_svg = crate::overlay::html_components::icons::OverlayIcon::Translate.svg(),
        expand_less_svg = crate::overlay::html_components::icons::OverlayIcon::ExpandLess.svg(),
        pip_svg = crate::overlay::html_components::icons::OverlayIcon::ResizeCorner.svg(),
        volume_up_svg = crate::overlay::html_components::icons::OverlayIcon::VolumeUp.svg(),
        apps_svg = crate::overlay::html_components::icons::OverlayIcon::Apps.svg(),
        download_svg = crate::overlay::html_components::icons::OverlayIcon::Download.svg(),
        close_svg = crate::overlay::html_components::icons::OverlayIcon::Close.svg(),
        dictionary_svg = crate::overlay::html_components::icons::OverlayIcon::Dictionary.svg(),
        vocabulary_title = text.realtime.realtime_tooltip_custom_vocabulary,
        vocabulary_prompt = text.realtime.realtime_custom_vocabulary_prompt,
        cancel_text = text.preset_basics.cancel_label,
    )
}

/// The user's custom vocabulary as an inline-script value. Entries are free
/// text, so they must not be able to change how the page's script element ends.
fn vocabulary_script_json(entries: &[String]) -> String {
    crate::overlay::utils::inline_script_json(
        &serde_json::to_string(entries).unwrap_or_else(|_| "[]".to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::vocabulary_script_json;

    #[test]
    fn custom_vocabulary_cannot_change_how_the_script_element_ends() {
        let entries = vec![
            "<!--".to_string(),
            "<script>".to_string(),
            "</SCRIPT>".to_string(),
        ];
        let json = vocabulary_script_json(&entries);
        assert!(
            !json.contains(['<', '>']),
            "vocabulary reached the inline script as markup: {json}"
        );
        assert_eq!(serde_json::from_str::<Vec<String>>(&json).unwrap(), entries);
    }
}
