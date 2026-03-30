package dev.screengoated.toolbox.mobile.service.overlay

private const val PRIMARY = "#5D5FEF"
private const val PRIMARY_LIGHT = "#B4B5FF"
private const val SECONDARY = "#2979FF"
private const val TERTIARY = "#F50057"
private const val SUCCESS = "#4CAF50"

internal fun overlayCss(
    baseCss: String,
    glowColor: String,
    fontSize: Int,
    isDark: Boolean,
): String {
    val palette = overlayPalette(glowColor, isDark)
    val replacements = linkedMapOf(
        "BG_COLOR" to palette.bgColor,
        "TEXT_COLOR" to palette.textColor,
        "BORDER_COLOR" to palette.borderColor,
        "BOX_SHADOW" to palette.boxShadow,
        "CTRL_BG" to palette.ctrlBg,
        "CTRL_BORDER" to palette.ctrlBorder,
        "CTRL_HOVER_BG" to palette.ctrlHoverBg,
        "CTRL_HOVER_TEXT" to palette.ctrlHoverText,
        "SELECT_BG" to palette.selectBg,
        "SELECT_OPTION_BG" to palette.selectOptionBg,
        "PLACEHOLDER_COLOR" to palette.placeholderColor,
        "RESIZE_HINT_COLOR" to palette.resizeHintColor,
        "SCROLLBAR_TRACK" to palette.scrollbarTrack,
        "SCROLLBAR_THUMB" to palette.scrollbarThumb,
        "SCROLLBAR_THUMB_HOVER" to palette.scrollbarThumbHover,
        "ICON_INACTIVE_COLOR" to palette.iconInactiveColor,
        "PRIMARY" to PRIMARY,
        "PRIMARY_LIGHT" to PRIMARY_LIGHT,
        "SECONDARY" to SECONDARY,
        "TERTIARY" to TERTIARY,
        "SUCCESS" to SUCCESS,
        "FONT_SIZE" to fontSize.toString(),
    )
    val resolvedBase = replacements.entries.fold(baseCss) { source, (token, value) ->
        source.replace("{{$token}}", value)
    }
    return buildString {
        append(resolvedBase)
        append('\n')
        append(overlayModalCss(isDark))
        append('\n')
        append(
            """
            #title:empty {
                display: none;
            }
            html, body, #header {
                backdrop-filter: none !important;
                -webkit-backdrop-filter: none !important;
            }
            html, body {
                background:
                    linear-gradient(180deg, rgba(255, 255, 255, 0.06), rgba(255, 255, 255, 0.02)),
                    ${palette.bgColor} !important;
                transform: translateZ(0);
            }
            #controls {
                min-width: 0;
                justify-content: flex-start;
                overflow-x: auto;
                overflow-y: hidden;
                flex-wrap: nowrap;
                padding-bottom: 2px;
                overflow-anchor: none;
                scroll-behavior: auto;
                scrollbar-width: none;
                -ms-overflow-style: none;
                -webkit-overflow-scrolling: touch;
                touch-action: pan-x;
            }
            #controls::-webkit-scrollbar {
                display: none;
            }
            #controls > *,
            .btn-group,
            .pill-group,
            .ctrl-btn,
            .vis-btn,
            .language-btn {
                flex-shrink: 0;
            }
            .ctrl-btn,
            .vis-btn,
            .audio-icon,
            .model-icon,
            .trans-model-icon,
            .language-btn {
                -webkit-tap-highlight-color: transparent;
            }
            .language-btn {
                font-family: 'Google Sans Flex', sans-serif;
                font-variation-settings: 'wght' 650, 'ROND' 100;
                font-size: 11px;
                color: ${palette.textColor};
                background: ${palette.selectBg};
                border: 1px solid ${palette.ctrlBorder};
                border-radius: 9999px;
                min-width: 46px;
                height: 28px;
                padding: 0 10px;
                display: inline-flex;
                align-items: center;
                justify-content: center;
            }
            .language-btn:hover,
            .language-btn:focus-visible {
                outline: none;
                border-color: ${PRIMARY}80;
                box-shadow: 0 2px 8px ${PRIMARY}30;
            }
            .text-chunk,
            .text-chunk.old,
            .text-chunk.new,
            .text-chunk.appearing,
            .text-chunk.appearing.show,
            .text-chunk.diff-updating,
            .text-chunk.commit-promoting {
                will-change: opacity, filter, transform, -webkit-mask-position, mask-position;
                backface-visibility: hidden;
                transform: translateZ(0);
            }
            #content {
                contain: layout paint style;
            }
            """.trimIndent(),
        )
    }
}

private fun overlayPalette(
    glowColor: String,
    isDark: Boolean,
): OverlayPalette {
    return if (isDark) {
        OverlayPalette(
            bgColor = "rgba(28, 27, 31, 0.95)",
            textColor = "#E6E1E5",
            borderColor = "${glowColor}50",
            ctrlBg = "rgba(44, 44, 44, 0.85)",
            ctrlHoverBg = "rgba(93, 95, 239, 0.2)",
            selectBg = "rgba(44, 44, 44, 0.95)",
            selectOptionBg = "#2c2c2c",
            placeholderColor = "#938F99",
            resizeHintColor = "#79747E",
            scrollbarTrack = "#2c2c2c",
            scrollbarThumb = "#49454F",
            scrollbarThumbHover = "#625D66",
            ctrlHoverText = "#E6E1E5",
            iconInactiveColor = "#79747E",
            boxShadow = "0 8px 32px rgba(0, 0, 0, 0.4), 0 0 0 1px ${glowColor}20",
            ctrlBorder = "rgba(255,255,255,0.08)",
        )
    } else {
        OverlayPalette(
            bgColor = "rgba(254, 247, 255, 0.95)",
            textColor = "#1C1B1F",
            borderColor = "${glowColor}40",
            ctrlBg = "rgba(234, 234, 234, 0.85)",
            ctrlHoverBg = "rgba(93, 95, 239, 0.12)",
            selectBg = "rgba(255, 255, 255, 0.95)",
            selectOptionBg = "#ffffff",
            placeholderColor = "#79747E",
            resizeHintColor = "#938F99",
            scrollbarTrack = "#f1f3f4",
            scrollbarThumb = "#CAC4D0",
            scrollbarThumbHover = "#AEA9B4",
            ctrlHoverText = "#1C1B1F",
            iconInactiveColor = "#938F99",
            boxShadow = "0 4px 16px rgba(0, 0, 0, 0.12), 0 0 0 1px ${glowColor}15",
            ctrlBorder = "rgba(0,0,0,0.08)",
        )
    }
}

private data class OverlayPalette(
    val bgColor: String,
    val textColor: String,
    val borderColor: String,
    val ctrlBg: String,
    val ctrlHoverBg: String,
    val selectBg: String,
    val selectOptionBg: String,
    val placeholderColor: String,
    val resizeHintColor: String,
    val scrollbarTrack: String,
    val scrollbarThumb: String,
    val scrollbarThumbHover: String,
    val ctrlHoverText: String,
    val iconInactiveColor: String,
    val boxShadow: String,
    val ctrlBorder: String,
)

private data class OverlayModalPalette(
    val bgColor: String,
    val textColor: String,
    val borderColor: String,
    val borderFocusColor: String,
    val labelColor: String,
    val sliderBg: String,
    val switchBg: String,
    val switchOnBg: String,
    val sliderThumb: String,
    val dividerColor: String,
    val shadowLg: String,
    val shadowSm: String,
)

private fun overlayModalCss(isDark: Boolean): String {
    val modal = if (isDark) {
        OverlayModalPalette(
            bgColor = "rgba(30, 30, 30, 0.98)",
            textColor = "#ccc",
            borderColor = "rgba(255, 150, 51, 0.5)",
            borderFocusColor = "#00c8ff80",
            labelColor = "#aaa",
            sliderBg = "#444",
            switchBg = "#444",
            switchOnBg = "#4caf50",
            sliderThumb = "#ff9633",
            dividerColor = "#555",
            shadowLg = "rgba(0,0,0,0.5)",
            shadowSm = "#ff963330",
        )
    } else {
        OverlayModalPalette(
            bgColor = "rgba(255, 255, 255, 0.98)",
            textColor = "#202124",
            borderColor = "rgba(255, 150, 51, 0.3)",
            borderFocusColor = "#00c8ff50",
            labelColor = "#5f6368",
            sliderBg = "#e0e0e0",
            switchBg = "#dadce0",
            switchOnBg = "#34a853",
            sliderThumb = "#fa7b17",
            dividerColor = "#dadce0",
            shadowLg = "rgba(0,0,0,0.15)",
            shadowSm = "#ff963320",
        )
    }
    return """
        #tts-modal, #download-modal {
            display: none;
            position: fixed !important;
            top: 50% !important;
            left: 50% !important;
            transform: translate(-50%, -50%) !important;
            background: ${modal.bgColor};
            border-radius: 12px;
            box-shadow: 0 8px 32px ${modal.shadowLg}, 0 0 20px ${modal.shadowSm};
            color: ${modal.textColor};
        }
        #tts-modal { padding: 16px 20px; }
        #download-modal {
            border: 1px solid ${modal.borderFocusColor};
            min-width: 320px;
            max-width: 90vw;
            padding: 12px 16px;
            text-align: center;
            z-index: 2147483647 !important;
        }
        #tts-modal {
            border: 1px solid ${modal.borderColor};
            min-width: 200px;
            z-index: 2147483647 !important;
        }
        #tts-modal.show, #download-modal.show {
            display: block !important;
            animation: modal-appear 0.2s ease-out;
        }
        #tts-modal-overlay, #download-modal-overlay {
            display: none;
            position: fixed !important;
            inset: 0;
            background: rgba(0,0,0,0.35);
        }
        #tts-modal-overlay.show, #download-modal-overlay.show {
            display: block !important;
        }
        #tts-modal-overlay, #download-modal-overlay {
            z-index: 2147483646 !important;
        }
        @keyframes modal-appear {
            from { opacity: 0; transform: translate(-50%, -50%) scale(0.9); }
            to { opacity: 1; transform: translate(-50%, -50%) scale(1); }
        }
        .tts-modal-title, .download-modal-title {
            display: flex;
            align-items: center;
            gap: 6px;
            font-size: 13px;
            font-weight: bold;
            margin-bottom: 10px;
        }
        .tts-modal-title { color: #ff9633; }
        .download-modal-title { color: #00c8ff; }
        .tts-modal-row {
            display: flex;
            align-items: center;
            justify-content: space-between;
            gap: 12px;
            margin-bottom: 12px;
        }
        .tts-modal-label, .download-modal-footnote {
            font-size: 11px;
            color: ${modal.labelColor};
        }
        .download-modal-msg {
            font-size: 11px;
            color: ${modal.textColor};
        }
        .toggle-switch {
            position: relative;
            width: 40px;
            height: 22px;
            background: ${modal.switchBg};
            border-radius: 11px;
            cursor: pointer;
            transition: background 0.2s;
        }
        .toggle-switch.on {
            background: ${modal.switchOnBg};
        }
        .toggle-switch::after {
            content: '';
            position: absolute;
            top: 2px;
            left: 2px;
            width: 18px;
            height: 18px;
            background: #fff;
            border-radius: 50%;
            transition: transform 0.2s;
        }
        .toggle-switch.on::after {
            transform: translateX(18px);
        }
        .speed-slider-container {
            display: flex;
            align-items: center;
            gap: 8px;
        }
        .speed-slider {
            -webkit-appearance: none;
            width: 100px;
            height: 6px;
            background: ${modal.sliderBg};
            border-radius: 3px;
            outline: none;
        }
        .speed-slider::-webkit-slider-thumb {
            -webkit-appearance: none;
            width: 14px;
            height: 14px;
            background: ${modal.sliderThumb};
            border-radius: 50%;
            cursor: pointer;
        }
        .speed-value {
            min-width: 36px;
            text-align: right;
            font-size: 11px;
            color: ${modal.sliderThumb};
            font-weight: bold;
        }
        .auto-toggle {
            padding: 4px 10px;
            font-size: 10px;
            border: 1px solid ${modal.dividerColor};
            border-radius: 12px;
            background: transparent;
            color: ${modal.labelColor};
            cursor: pointer;
        }
        .auto-toggle.on {
            background: linear-gradient(135deg, #ff9633 0%, #ff6b00 100%);
            border-color: #ff9633;
            color: #fff;
        }
        .download-progress-bar {
            width: 100%;
            height: 6px;
            background: ${modal.sliderBg};
            border-radius: 3px;
            overflow: hidden;
            margin-bottom: 8px;
        }
        .download-progress-fill {
            height: 100%;
            width: 0%;
            background: linear-gradient(90deg, #00c8ff, #0080ff);
            transition: width 0.2s ease;
        }
        .download-cancel-btn {
            display: flex;
            align-items: center;
            justify-content: center;
            gap: 4px;
            margin-top: 12px;
            padding: 8px 16px;
            width: 100%;
            border: 1px solid #ff4444;
            border-radius: 6px;
            background: transparent;
            color: #ff6666;
            font-size: 11px;
            cursor: pointer;
        }
    """.trimIndent()
}
