pub(super) fn get(
    icon_inactive_color: &str,
    primary: &str,
    secondary: &str,
    tertiary: &str,
) -> String {
    format!(
        r###"        .audio-icon {{
            font-size: 22px;
            padding: 0;
            cursor: pointer;
            color: {icon_inactive_color};
            transition: all 0.25s cubic-bezier(0.2, 0.0, 0, 1.0);
            background: transparent;
            border: none;
        }}
        .audio-icon:hover {{
            color: {secondary}80;
            transform: scale(1.08);
        }}
        .audio-icon.active {{
            color: {secondary};
        }}
        .model-icon {{
            font-size: 22px;
            padding: 0;
            cursor: pointer;
            color: {icon_inactive_color};
            transition: all 0.25s cubic-bezier(0.2, 0.0, 0, 1.0);
            background: transparent;
            border: none;
        }}
        .model-icon:hover {{
            color: {tertiary}80;
            transform: scale(1.08);
        }}
        .model-icon.active {{
            color: {tertiary};
        }}
        @keyframes model-switch-pulse {{
            0% {{ transform: scale(1); box-shadow: 0 0 0 0 {tertiary}B0; }}
            25% {{ transform: scale(1.25); box-shadow: 0 0 12px 4px {tertiary}70; }}
            50% {{ transform: scale(1.1); box-shadow: 0 0 8px 2px {tertiary}40; }}
            75% {{ transform: scale(1.15); box-shadow: 0 0 10px 3px {tertiary}50; }}
            100% {{ transform: scale(1); box-shadow: 0 0 0 0 {tertiary}00; }}
        }}
        .model-icon.switching {{
            animation: model-switch-pulse 2s cubic-bezier(0.2, 0.0, 0, 1.0);
            color: {tertiary} !important;
            background: {tertiary}30 !important;
            border-radius: 6px;
        }}

        /* Transcription Model Icons */
        .trans-model-icon {{
            font-size: 22px;
            padding: 0;
            cursor: pointer;
            color: {icon_inactive_color};
            transition: all 0.25s cubic-bezier(0.2, 0.0, 0, 1.0);
            background: transparent;
            border: none;
        }}
        .trans-model-icon:hover {{
            transform: scale(1.08);
        }}
        .trans-model-icon.active[data-value="google-gemini-2-5-live-transcribe-audio"] {{
            color: {secondary};
        }}
        .trans-model-icon.active[data-value="google-gemini-3-1-live-transcribe-audio"] {{
            color: {secondary};
        }}
        .trans-model-icon.active[data-value="google-gemini-3-5-live-translate-audio"] {{
            color: {secondary};
        }}
        .trans-model-icon.active[data-value="parakeet"] {{
            color: {primary};
        }}
        .trans-model-icon.active[data-value="local-qwen-3-asr-600m-audio"] {{
            color: {tertiary};
        }}
        .trans-model-icon.active[data-value="local-qwen-3-asr-1-7b-audio"] {{
            color: {tertiary};
        }}
        .trans-model-icon.active[data-value="zipformer"] {{
            color: {tertiary};
        }}

        /* Speak button styling */
        .speak-btn {{
            position: relative;
        }}
        .speak-btn.active {{
            color: #4caf50 !important;
            border-color: #4caf50;
            background: rgba(76, 175, 80, 0.14);
            box-shadow: 0 2px 8px rgba(76, 175, 80, 0.35);
        }}
        .speak-btn.active .inline-svg-icon {{
            animation: speak-pulse 1.5s cubic-bezier(0.2, 0.0, 0, 1.0) infinite;
        }}
        .speak-btn.locked {{
            cursor: default;
        }}
        @keyframes speak-pulse {{
            0%, 100% {{ opacity: 1; }}
            50% {{ opacity: 0.5; }}
        }}
"###,
        icon_inactive_color = icon_inactive_color,
        primary = primary,
        secondary = secondary,
        tertiary = tertiary,
    )
}
