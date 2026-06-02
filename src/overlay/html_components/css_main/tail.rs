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
        .trans-model-icon.active[data-value="gemini-live-audio"] {{
            color: {secondary};
        }}
        .trans-model-icon.active[data-value="parakeet"] {{
            color: {primary};
        }}
        .trans-model-icon.active[data-value="qwen3-asr-0.6b"] {{
            color: {tertiary};
        }}

        /* Waveform animation for listening state */
        .wave-line {{
             transform-box: fill-box;
             transform-origin: center;
             animation: wave-animation 1.2s cubic-bezier(0.2, 0.0, 0, 1.0) infinite;
        }}
        .wave-line.delay-1 {{ animation-delay: 0s; }}
        .wave-line.delay-2 {{ animation-delay: 0.15s; }}
        .wave-line.delay-3 {{ animation-delay: 0.3s; }}
        .wave-line.delay-4 {{ animation-delay: 0.1s; }}

        @keyframes wave-animation {{
            0%, 100% {{
                transform: scaleY(1);
            }}
            50% {{
                transform: scaleY(1.7);
            }}
        }}

        /* Translation animation */
        .trans-part-1 {{
            animation: lang-bounce 2.2s cubic-bezier(0.2, 0.0, 0, 1.0) infinite;
        }}
        .trans-part-2 {{
            animation: lang-bounce 2.2s cubic-bezier(0.2, 0.0, 0, 1.0) infinite;
            animation-delay: 1.1s;
        }}
        @keyframes lang-bounce {{
            0%, 100% {{ transform: translateY(0); opacity: 0.8; }}
            50% {{ transform: translateY(-2px); opacity: 1; }}
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
        .speak-btn.active .material-symbols-rounded {{
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
