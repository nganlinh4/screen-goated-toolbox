pub(super) fn get(icon_inactive_color: &str, secondary: &str) -> String {
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
        secondary = secondary,
    )
}
