use crate::gui::locale::LocaleText;

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

    let _mic_text = text.realtime_mic;
    let _device_text = text.realtime_device;
    let placeholder_text = text.realtime_waiting;

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
            let lang_code = isolang::Language::from_name(lang)
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
            let gemini_id = crate::model_config::GEMINI_LIVE_AUDIO_MODEL_ID_2_5;
            let gemini_3_1_id = crate::model_config::GEMINI_LIVE_AUDIO_MODEL_ID_3_1;
            let qwen3_0_6b_id = crate::model_config::QWEN3_ASR_0_6B_MODEL_ID;

            // Build transcription model dropdown options from the shared model catalog.
            let qwen3_1_7b_id = crate::model_config::QWEN3_ASR_1_7B_MODEL_ID;
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

            // Build transcription language dropdown (only active for zipformer)
            let trans_lang_code = {
                let app = crate::APP.lock().unwrap();
                app.config.realtime_transcription_language.clone()
            };
            let is_all_lang = transcription_model == gemini_id
                || transcription_model == gemini_3_1_id
                || crate::model_config::is_gemini_live_s2s_model_id(transcription_model)
                || transcription_model == qwen3_0_6b_id
                || transcription_model == qwen3_1_7b_id;
            let is_en_only = transcription_model == "parakeet";
            let mut effective_lang = if is_all_lang {
                "all"
            } else if is_en_only {
                "en"
            } else {
                &trans_lang_code
            };
            if transcription_model == "zipformer" && effective_lang == "all" {
                effective_lang = "en";
            }
            let zipformer_lang_options = [
                ("en", "English"),
                ("ko", "Korean"),
                ("zh", "Chinese"),
                ("fr", "French"),
                ("de", "German"),
                ("es", "Spanish"),
                ("ru", "Russian"),
                ("all-8", "AR,EN,ID,JA,RU,TH,VI,ZH"),
            ];
            let non_zipformer_lang_options = [
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
            let trans_lang_options: &[(&str, &str)] = if transcription_model == "zipformer" {
                &zipformer_lang_options
            } else {
                &non_zipformer_lang_options
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
            let trans_lang_disabled = if transcription_model == "zipformer" {
                ""
            } else {
                "disabled"
            };
            let trans_lang_hidden = if transcription_model == "zipformer" {
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
                <select class="model-dropdown" id="transcription-lang-select" title="{language_title}" {trans_lang_disabled} {trans_lang_hidden}>
                    {trans_lang_html}
                </select>
            "#,
                mic_active = if !is_device { "active" } else { "" },
                device_active = if is_device { "active" } else { "" },
                mic_svg = crate::overlay::html_components::icons::get_icon_svg("mic"),
                device_svg = crate::overlay::html_components::icons::get_icon_svg("speaker_group"),
                mic_title = text.realtime_tooltip_microphone_input,
                device_title = text.realtime_tooltip_device_audio,
                model_title = text.realtime_tooltip_transcription_model,
                language_title = text.realtime_tooltip_transcription_language,
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

            let trans_model_options = [(llm_id, text.llm_label), (gtx_id, text.google_gtx_label)];
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
                    text.realtime_tooltip_s2s_translation_model
                } else {
                    text.realtime_tooltip_translation_model
                },
                translation_model_disabled = if is_s2s { "disabled" } else { "" },
                translation_model_hidden = if is_s2s { "hidden" } else { "" },
                speak_active = if is_s2s { "active locked" } else { "" },
                speak_title = if is_s2s {
                    text.realtime_tooltip_direct_speech
                } else {
                    text.realtime_tooltip_tts_settings
                },
                language_title = if is_s2s {
                    text.realtime_tooltip_s2s_target_language
                } else {
                    text.realtime_tooltip_target_language
                },
                language_disabled = "",
                volume_up_svg = crate::overlay::html_components::icons::get_icon_svg("volume_up"),
            )
        }
    };

    let loading_icon = if is_translation {
        r##"<svg class="loading-svg" viewBox="0 -6 24 36" fill="none" stroke="#ff9633" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><g class="trans-part-1"><path d="m5 8 6 6"></path><path d="m4 14 6-6 2-3"></path><path d="M2 5h12"></path><path d="M7 2h1"></path></g><g class="trans-part-2"><path d="m22 22-5-10-5 10"></path><path d="M14 18h6"></path></g></svg>"##
    } else {
        r##"<svg class="loading-svg" viewBox="0 -12 24 48" fill="none" stroke="#00c8ff" stroke-width="4" stroke-linecap="round" stroke-linejoin="round"><line class="wave-line delay-1" x1="4" y1="8" x2="4" y2="16"></line><line class="wave-line delay-2" x1="9" y1="4" x2="9" y2="20"></line><line class="wave-line delay-3" x1="14" y1="6" x2="14" y2="18"></line><line class="wave-line delay-4" x1="19" y1="8" x2="19" y2="16"></line></svg>"##
    };

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
        "translationModel": text.realtime_tooltip_translation_model,
        "s2sTranslationModel": text.realtime_tooltip_s2s_translation_model,
        "targetLanguage": text.realtime_tooltip_target_language,
        "s2sTargetLanguage": text.realtime_tooltip_s2s_target_language,
        "directSpeech": text.realtime_tooltip_direct_speech,
        "ttsSettings": text.realtime_tooltip_tts_settings,
        "ttsS2sLocked": text.realtime_tts_s2s_locked_tooltip,
        "ttsEnable": text.realtime_tts_enable_tooltip,
    })
    .to_string();

    // Get local font CSS (cached fonts, no network loading)
    let font_css = crate::overlay::html_components::font_manager::get_font_css();

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
        window.REALTIME_L10N = {l10n_json};
        {js_content}
    </script>
</body>
</html>"#,
        css_content = css,
        js_content = js,
        l10n_json = l10n_json,
        is_s2s_attr = if is_s2s { "1" } else { "0" },
        is_live_translate_attr = if is_live_translate { "1" } else { "0" },
        tts_toggle_class = if is_s2s { "on locked" } else { "" },
        tts_toggle_title = if is_s2s {
            text.realtime_tts_s2s_locked_tooltip
        } else {
            text.realtime_tts_enable_tooltip
        },
        loading_icon = loading_icon,
        title_content = title_content,
        audio_selector = audio_selector,
        placeholder_text = placeholder_text,
        copy_title = text.realtime_tooltip_copy_text,
        font_decrease_title = text.realtime_tooltip_decrease_font,
        font_increase_title = text.realtime_tooltip_increase_font,
        toggle_mic_title = text.toggle_transcription_tooltip,
        toggle_trans_title = text.toggle_translation_tooltip,
        toggle_header_title = text.realtime_tooltip_toggle_header,
        download_default_title = text.realtime_download_default_title,
        download_wait = text.realtime_download_wait,
        download_cancel_title = text.realtime_download_cancel_tooltip,
        tts_auto_title = text.realtime_tts_auto_tooltip,
        app_loading = text.realtime_app_loading,
        tts_title = text.realtime_tts_title,
        tts_speed = text.realtime_tts_speed,
        tts_auto = text.realtime_tts_auto,
        tts_volume = text.realtime_tts_volume,
        app_select_title = text.app_select_title,
        app_select_hint = text.app_select_hint,
        content_copy_svg = crate::overlay::html_components::icons::get_icon_svg("content_copy"),
        remove_svg = crate::overlay::html_components::icons::get_icon_svg("remove"),
        add_svg = crate::overlay::html_components::icons::get_icon_svg("add"),
        subtitles_svg = crate::overlay::html_components::icons::get_icon_svg("subtitles"),
        translate_svg = crate::overlay::html_components::icons::get_icon_svg("translate"),
        expand_less_svg = crate::overlay::html_components::icons::get_icon_svg("expand_less"),
        pip_svg = crate::overlay::html_components::icons::get_icon_svg("resize_corner"),
        volume_up_svg = crate::overlay::html_components::icons::get_icon_svg("volume_up"),
        apps_svg = crate::overlay::html_components::icons::get_icon_svg("apps"),
        download_svg = crate::overlay::html_components::icons::get_icon_svg("download"),
        close_svg = crate::overlay::html_components::icons::get_icon_svg("close"),
        cancel_text = text.cancel_label,
    )
}
