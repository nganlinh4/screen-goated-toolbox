pub fn get(glow_color: &str, font_size: u32, is_dark: bool) -> String {
    // Material Design 3 inspired palette matching the system aesthetic
    let (
        bg_color,
        text_color,
        border_color,
        ctrl_bg,
        ctrl_hover_bg,
        select_bg,
        select_option_bg,
        placeholder_color,
        resize_hint_color,
        scrollbar_track,
        scrollbar_thumb,
        scrollbar_thumb_hover,
        ctrl_hover_text,
        icon_inactive_color,
        surface_tint,
    ) = if is_dark {
        (
            "rgba(28, 27, 31, 0.95)",    // bg_color - MD3 dark surface
            "#E6E1E5",                   // text_color - MD3 on-surface
            format!("{}50", glow_color), // border_color
            "rgba(44, 44, 44, 0.85)",    // ctrl_bg - #2c2c2c with alpha
            "rgba(93, 95, 239, 0.2)",    // ctrl_hover_bg - primary with alpha
            "rgba(44, 44, 44, 0.95)",    // select_bg
            "#2c2c2c",                   // select_option_bg
            "#938F99",                   // placeholder_color - MD3 on-surface-variant
            "#79747E",                   // resize_hint_color - MD3 outline
            "#2c2c2c",                   // scrollbar_track
            "#49454F",                   // scrollbar_thumb - MD3 outline-variant
            "#625D66",                   // scrollbar_thumb_hover
            "#E6E1E5",                   // ctrl_hover_text
            "#79747E",                   // icon_inactive_color
            "rgba(93, 95, 239, 0.08)",   // surface_tint - primary overlay
        )
    } else {
        (
            "rgba(254, 247, 255, 0.95)", // bg_color - MD3 light surface
            "#1C1B1F",                   // text_color - MD3 on-surface
            format!("{}40", glow_color), // border_color
            "rgba(234, 234, 234, 0.85)", // ctrl_bg - matches tray_popup hover
            "rgba(93, 95, 239, 0.12)",   // ctrl_hover_bg - primary with alpha
            "rgba(255, 255, 255, 0.95)", // select_bg
            "#ffffff",                   // select_option_bg
            "#79747E",                   // placeholder_color - MD3 on-surface-variant
            "#938F99",                   // resize_hint_color
            "#f1f3f4",                   // scrollbar_track
            "#CAC4D0",                   // scrollbar_thumb - MD3 outline
            "#AEA9B4",                   // scrollbar_thumb_hover
            "#1C1B1F",                   // ctrl_hover_text
            "#938F99",                   // icon_inactive_color
            "rgba(93, 95, 239, 0.05)",   // surface_tint
        )
    };

    // Softer shadow matching system glassmorphism style
    let box_shadow = if is_dark {
        format!(
            "0 8px 32px rgba(0, 0, 0, 0.4), 0 0 0 1px {}20",
            glow_color
        )
    } else {
        format!(
            "0 4px 16px rgba(0, 0, 0, 0.12), 0 0 0 1px {}15",
            glow_color
        )
    };

    let ctrl_border = if is_dark {
        "rgba(255,255,255,0.08)"
    } else {
        "rgba(0,0,0,0.08)"
    };

    // MD3 accent colors
    let primary = "#5D5FEF";       // Purple - main accent
    let primary_light = "#B4B5FF"; // Light purple
    let secondary = "#2979FF";     // Blue
    let tertiary = "#F50057";      // Pink
    let success = "#4CAF50";       // Green for success states

    let _ = surface_tint; // Used for future surface tint effects

    format!(
        r###"        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        html, body {{
            height: 100%;
            overflow: hidden;
            background: {bg_color};
            font-family: 'Google Sans Flex', sans-serif;
            color: {text_color};
            border-radius: 12px;
            border: 1px solid {border_color};
            box-shadow: {box_shadow};
            backdrop-filter: blur(16px);
            -webkit-backdrop-filter: blur(16px);
        }}
        /* Loading overlay - TEMPORARILY DISABLED FOR TESTING */
        #loading-overlay {{
            display: none; /* TEMP: Remove this line to re-enable overlay */
            position: fixed;
            top: 0;
            left: 0;
            right: 0;
            bottom: 0;
            background: {bg_color};
            z-index: 9999;
            pointer-events: none;
            justify-content: center;
            align-items: center;
            animation: fadeOut 0.35s cubic-bezier(0.2, 0.0, 0, 1.0) 0.9s forwards;
        }}
        .loading-svg {{
            width: 72px;
            height: 72px;
            filter: drop-shadow(0 0 12px {primary}90);
            animation: breathe 2.5s ease-in-out infinite;
        }}
        @keyframes breathe {{
            0%, 100% {{
                transform: scale(1);
                opacity: 0.85;
                filter: drop-shadow(0 0 8px {primary}60);
            }}
            50% {{
                transform: scale(1.08);
                opacity: 1;
                filter: drop-shadow(0 0 20px {primary});
            }}
        }}
        @keyframes fadeOut {{
            from {{ opacity: 1; }}
            to {{ opacity: 0; }}
        }}
        .material-symbols-rounded {{
            font-family: 'Material Symbols Rounded'; /* Fallback */
            font-weight: normal;
            font-style: normal;
            font-size: 24px;
            line-height: 1;
            letter-spacing: normal;
            text-transform: none;
            display: inline-flex; /* Center SVG */
            align-items: center;
            justify-content: center;
            white-space: nowrap;
            word-wrap: normal;
            direction: ltr;
            vertical-align: middle;
            
            /* SVG container sizing */
            width: 1em;
            height: 1em;
        }}
        .material-symbols-rounded svg {{
            width: 100%;
            height: 100%;
            fill: currentColor;
            display: block;
        }}
        #container {{
            display: flex;
            flex-direction: column;
            height: 100%;
            padding: 8px 12px;
            cursor: grab;
            position: relative;
        }}
        #container:active {{
            cursor: grabbing;
        }}
        #header {{
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-bottom: 6px;
            flex-shrink: 0;
            gap: 8px;
            transition: all 0.3s cubic-bezier(0.2, 0.0, 0, 1.0);
            overflow: hidden;
            max-height: 40px;
            backdrop-filter: blur(16px);
            -webkit-backdrop-filter: blur(16px);
            border-radius: 8px;
        }}
        #header.collapsed {{
            max-height: 0;
            margin-bottom: 0;
            opacity: 0;
        }}
        @keyframes pulse {{
            0%, 100% {{ transform: translateX(-50%) scale(1); opacity: 0.7; }}
            50% {{ transform: translateX(-50%) scale(1.15); opacity: 1; }}
        }}
        #header-toggle {{
            position: absolute;
            left: 50%;
            transform: translateX(-50%);
            display: flex;
            justify-content: center;
            align-items: center;
            cursor: pointer;
            padding: 2px 6px;
            color: {resize_hint_color};
            transition: all 0.3s cubic-bezier(0.2, 0.0, 0, 1.0);
            z-index: 10;
            top: 32px;
            opacity: 0.4;
        }}
        #header:hover ~ #header-toggle {{
            color: {primary};
            opacity: 1;
            animation: pulse 1.2s ease-in-out infinite;
        }}
        #header-toggle:hover {{
            color: {primary_light};
            opacity: 1;
            animation: pulse 1s ease-in-out infinite;
        }}
        #header-toggle.collapsed {{
            top: 4px;
            opacity: 0.3;
            animation: none;
        }}
        #header-toggle.collapsed:hover {{
            opacity: 0.8;
        }}
        #header-toggle .material-symbols-rounded {{
            font-size: 14px;
            transition: transform 0.3s cubic-bezier(0.2, 0.0, 0, 1.0);
        }}
        #header-toggle.collapsed .material-symbols-rounded {{
            transform: rotate(180deg);
        }}
        #title {{
            font-size: 12px;
            font-weight: bold;
            color: {placeholder_color};
            flex-shrink: 0;
            display: flex;
            align-items: center;
            gap: 6px;
        }}
        #volume-canvas {{
            height: 24px;
            width: 90px;
            border-radius: 2px;
        }}
        #controls {{
            position: relative;
            z-index: 50;
            display: flex;
            gap: 8px;
            align-items: center;
            flex: 1;
            justify-content: flex-end;
        }}
        .btn-group {{
            display: flex;
            gap: 1px;
            align-items: center;
        }}
        .ctrl-btn {{
            font-size: 20px;
            color: {resize_hint_color};
            cursor: pointer;
            padding: 2px;
            border-radius: 9999px;
            background: {ctrl_bg};
            border: 1px solid {ctrl_border};
            transition: all 0.25s cubic-bezier(0.2, 0.0, 0, 1.0);
            user-select: none;
            width: 28px;
            height: 28px;
            display: flex;
            align-items: center;
            justify-content: center;
        }}
        .ctrl-btn:hover {{
            color: {ctrl_hover_text};
            background: {ctrl_hover_bg};
            border-color: {primary}80;
            box-shadow: 0 2px 8px {primary}30;
            transform: scale(1.05);
        }}
        .ctrl-btn.copied {{
            color: {success} !important;
            border-color: {success};
            box-shadow: 0 2px 8px {success}40;
        }}
        .pill-group {{
            display: flex;
            align-items: center;
            background: {ctrl_bg};
            border: 1px solid {ctrl_border};
            border-radius: 9999px;
            padding: 3px;
            gap: 2px;
            transition: all 0.25s cubic-bezier(0.2, 0.0, 0, 1.0);
        }}
        .pill-group:hover {{
            border-color: {primary}40;
            box-shadow: 0 2px 12px {primary}15;
        }}
        .pill-group .ctrl-btn {{
            background: transparent;
            border: none;
            width: 24px;
            height: 24px;
        }}
        .pill-group .ctrl-btn:hover {{
            background: {ctrl_hover_bg};
            box-shadow: none;
            transform: none;
        }}
        .vis-btn {{
            font-size: 20px;
            cursor: pointer;
            padding: 2px;
            border-radius: 6px;
            transition: all 0.25s cubic-bezier(0.2, 0.0, 0, 1.0);
            user-select: none;
            background: transparent;
            border: none;
        }}
        .vis-btn.active {{
            opacity: 1;
        }}
        .vis-btn.inactive {{
            opacity: 0.35;
        }}
        .vis-btn:hover {{
            opacity: 0.75;
            transform: scale(1.08);
        }}
        .vis-btn.mic {{
            color: {secondary};
        }}
        .vis-btn.trans {{
            color: {tertiary};
        }}
        select {{
            font-family: 'Google Sans Flex', sans-serif;
            font-variation-settings: 'wght' 600, 'ROND' 100;
            background: {select_bg};
            color: {text_color};
            border: 1px solid {ctrl_border};
            border-radius: 9999px;
            padding: 0;
            font-size: 10px;
            font-weight: bold;
            cursor: pointer;
            outline: none;
            width: 28px;
            height: 28px;
            scrollbar-width: thin;
            scrollbar-color: {scrollbar_thumb} {scrollbar_track};
            transition: all 0.25s cubic-bezier(0.2, 0.0, 0, 1.0);
            -webkit-appearance: none;
            -moz-appearance: none;
            appearance: none;
            text-align: center;
            text-align-last: center;
        }}
        select:hover {{
            border-color: {primary}80;
            box-shadow: 0 2px 8px {primary}25;
        }}
        select option {{
            font-family: 'Google Sans Flex', sans-serif;
            background: {select_option_bg};
            color: {text_color};
            padding: 4px 8px;
        }}
        select option:checked {{
            background: linear-gradient(0deg, {primary}40, {primary}40);
        }}
        /* Custom scrollbar for WebKit browsers */
        select::-webkit-scrollbar {{
            width: 6px;
        }}
        select::-webkit-scrollbar-track {{
            background: {scrollbar_track};
            border-radius: 3px;
        }}
        select::-webkit-scrollbar-thumb {{
            background: {scrollbar_thumb};
            border-radius: 3px;
        }}
        select::-webkit-scrollbar-thumb:hover {{
            background: {scrollbar_thumb_hover};
        }}
        #viewport {{
            flex: 1;
            overflow: hidden;
            position: relative;
        }}
        #content {{
            font-size: {font_size}px;
            line-height: 1.5;
            padding-bottom: 5px;
        }}
        @keyframes wipe-in {{
            from {{
                -webkit-mask-position: 100% 0;
                mask-position: 100% 0;
                transform: translateX(-4px);
                opacity: 0;
                filter: blur(2px);
            }}
            to {{
                -webkit-mask-position: 0% 0;
                mask-position: 0% 0;
                transform: translateX(0);
                opacity: 1;
                filter: blur(0);
            }}
        }}

        /* Base styling for all text chunks */
        .text-chunk {{
            font-family: 'Google Sans Flex', sans-serif !important;
            font-optical-sizing: auto;
            display: inline;
            transition:
                color 0.5s cubic-bezier(0.2, 0.0, 0, 1.0),
                font-variation-settings 0.5s cubic-bezier(0.2, 0.0, 0, 1.0),
                -webkit-mask-position 0.4s cubic-bezier(0.2, 0.0, 0, 1.0),
                mask-position 0.4s cubic-bezier(0.2, 0.0, 0, 1.0),
                opacity 0.35s cubic-bezier(0.2, 0.0, 0, 1.0),
                filter 0.35s cubic-bezier(0.2, 0.0, 0, 1.0);
        }}

        /* Old/committed text styling */
        .text-chunk.old {{
            color: {placeholder_color};
            font-variation-settings: 'wght' 300, 'wdth' 100, 'slnt' 0, 'GRAD' 0, 'ROND' 100, 'ROUN' 100, 'RNDS' 100;
        }}

        /* New/uncommitted text styling */
        .text-chunk.new {{
            color: {text_color};
            font-variation-settings: 'wght' 350, 'wdth' 99, 'slnt' 0, 'GRAD' 150, 'ROND' 100, 'ROUN' 100, 'RNDS' 100;
        }}

        /* Appearing state - wipe animation */
        .text-chunk.appearing {{
            color: {text_color};
            font-variation-settings: 'wght' 350, 'wdth' 99, 'slnt' 0, 'GRAD' 150, 'ROND' 100, 'ROUN' 100, 'RNDS' 100;

            -webkit-mask-image: linear-gradient(to right, black 50%, transparent 100%);
            mask-image: linear-gradient(to right, black 50%, transparent 100%);
            -webkit-mask-size: 200% 100%;
            mask-size: 200% 100%;
            -webkit-mask-position: 100% 0;
            mask-position: 100% 0;
            opacity: 0;
            filter: blur(2px);
        }}

        /* Appearing -> visible */
        .text-chunk.appearing.show {{
            -webkit-mask-position: 0% 0;
            mask-position: 0% 0;
            opacity: 1;
            filter: blur(0);
        }}
        .placeholder {{
            color: {placeholder_color};
            font-style: italic;
        }}
        /* Resize handle - visible grip in corner */
         #resize-hint {{
             position: absolute;
             bottom: 0;
             right: 0;
             width: 16px;
             height: 16px;
             cursor: se-resize;
             opacity: 0.25;
             display: flex;
             align-items: flex-end;
             justify-content: flex-end;
             padding: 2px;
             font-size: 10px;
             color: {resize_hint_color};
             user-select: none;
             transition: all 0.25s cubic-bezier(0.2, 0.0, 0, 1.0);
         }}
        #resize-hint:hover {{
             opacity: 1;
             color: {primary};
         }}
        .audio-icon {{
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
        .trans-model-icon.active[data-value="gemini"] {{
            color: {secondary};
        }}
        .trans-model-icon.active[data-value="parakeet"] {{
            color: {primary};
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
            color: {success} !important;
            border-color: {success};
            box-shadow: 0 2px 8px {success}40;
        }}
        .speak-btn.active .material-symbols-rounded {{
            animation: speak-pulse 1.5s cubic-bezier(0.2, 0.0, 0, 1.0) infinite;
        }}
        @keyframes speak-pulse {{
            0%, 100% {{ opacity: 1; }}
            50% {{ opacity: 0.5; }}
        }}
        "###,
        bg_color = bg_color,
        text_color = text_color,
        border_color = border_color,
        box_shadow = box_shadow,
        font_size = font_size,
        ctrl_bg = ctrl_bg,
        ctrl_border = ctrl_border,
        select_bg = select_bg,
        select_option_bg = select_option_bg,
        scrollbar_thumb = scrollbar_thumb,
        scrollbar_track = scrollbar_track,
        scrollbar_thumb_hover = scrollbar_thumb_hover,
        placeholder_color = placeholder_color,
        resize_hint_color = resize_hint_color,
        ctrl_hover_bg = ctrl_hover_bg,
        ctrl_hover_text = ctrl_hover_text,
        icon_inactive_color = icon_inactive_color,
        primary = primary,
        primary_light = primary_light,
        secondary = secondary,
        tertiary = tertiary,
        success = success,
    )
}
