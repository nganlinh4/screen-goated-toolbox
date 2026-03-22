package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import dev.screengoated.toolbox.mobile.service.overlay.overlayBridgePrelude
import dev.screengoated.toolbox.mobile.service.overlay.overlayFontCss
import kotlin.math.roundToInt

internal data class PresetAudioCaptureHtmlSettings(
    val uiLanguage: String,
    val isDark: Boolean,
    val windowWidth: Int,
    val windowHeight: Int,
)

internal class PresetAudioCaptureHtmlBuilder(
    private val context: Context,
) {
    private val template by lazy { asset("windows_recording_template.html") }
    private val density = context.resources.displayMetrics.density

    fun build(settings: PresetAudioCaptureHtmlSettings): String {
        val palette = if (settings.isDark) {
            AudioRecordingPalette(
                containerBg = "rgba(18, 18, 18, 0.85)",
                containerBorder = "rgba(255, 255, 255, 0.1)",
                text = "white",
                subtext = "rgba(255, 255, 255, 0.7)",
                buttonBg = "rgba(255, 255, 255, 0.05)",
                buttonHoverBg = "rgba(255, 255, 255, 0.15)",
                button = "rgba(255, 255, 255, 0.8)",
                textShadow = "0 1px 2px rgba(0, 0, 0, 0.3)",
            )
        } else {
            AudioRecordingPalette(
                containerBg = "rgba(255, 255, 255, 0.92)",
                containerBorder = "rgba(0, 0, 0, 0.1)",
                text = "#222222",
                subtext = "rgba(0, 0, 0, 0.6)",
                buttonBg = "rgba(0, 0, 0, 0.05)",
                buttonHoverBg = "rgba(0, 0, 0, 0.1)",
                button = "rgba(0, 0, 0, 0.7)",
                textShadow = "0 1px 2px rgba(255, 255, 255, 0.3)",
            )
        }
        val copy = audioLocale(settings.uiLanguage)
        val cssWidth = ((settings.windowWidth / density) - WINDOW_CHROME_INSET_CSS)
            .roundToInt()
            .coerceAtLeast(1)
        val cssHeight = ((settings.windowHeight / density) - WINDOW_CHROME_INSET_CSS)
            .roundToInt()
            .coerceAtLeast(1)
        val replacements = linkedMapOf(
            "FONT_CSS" to overlayFontCss(),
            "WINDOW_WIDTH" to cssWidth.toString(),
            "WINDOW_HEIGHT" to cssHeight.toString(),
            "TEXT_RECORDING" to copy.recording,
            "TEXT_PROCESSING" to copy.processing,
            "TEXT_WARMUP" to copy.warmup,
            "TEXT_INITIALIZING" to copy.initializing,
            "TEXT_SUBTEXT" to copy.subtext,
            "TEXT_PAUSED" to copy.paused,
            "COLOR_CONTAINER_BG" to palette.containerBg,
            "COLOR_CONTAINER_BORDER" to palette.containerBorder,
            "COLOR_TEXT" to palette.text,
            "COLOR_SUBTEXT" to palette.subtext,
            "COLOR_BUTTON_BG" to palette.buttonBg,
            "COLOR_BUTTON_HOVER_BG" to palette.buttonHoverBg,
            "COLOR_BUTTON" to palette.button,
            "COLOR_TEXT_SHADOW" to palette.textShadow,
            "IS_DARK" to if (settings.isDark) "true" else "false",
            "BRIDGE_PRELUDE" to overlayBridgePrelude(),
            "MOBILE_SHIM" to audioRecordingMobileShim(),
        )
        val resolved = replacements.entries.fold(template) { html, (token, value) ->
            html.replace("{{$token}}", value)
        }
        return injectViewportMeta(resolved)
    }

    private fun asset(name: String): String {
        return context.assets.open("preset_overlay/$name").bufferedReader().use { it.readText() }
    }
}

private fun injectViewportMeta(html: String): String {
    val viewport = """
        <meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no, viewport-fit=cover" />
        <meta name="format-detection" content="telephone=no" />
        <style>
            html, body {
                -webkit-text-size-adjust: 100%;
                text-size-adjust: 100%;
            }
        </style>
    """.trimIndent()
    return html.replace("<head>", "<head>\n$viewport\n")
}

internal fun audioRecordingWindowDimensions(
    screenWidth: Int,
    screenHeight: Int,
    density: Float,
): Pair<Int, Int> {
    val safeHeight = screenHeight.coerceAtLeast(1)
    val safeDensity = density.coerceAtLeast(1f)
    val screenCssWidth = screenWidth / safeDensity
    val screenCssHeight = safeHeight / safeDensity
    val aspectRatio = screenCssWidth.toDouble() / screenCssHeight.toDouble()
    val targetCssWidth = (450.0 - (aspectRatio - BASE_ASPECT_RATIO) * 127.0)
        .coerceIn(350.0, 500.0)
        .toFloat()
    val availableCssWidth = (screenCssWidth - WINDOW_SIDE_MARGIN_CSS * 2).coerceAtLeast(MIN_WINDOW_WIDTH_CSS)
    val outerCssWidth = targetCssWidth.coerceAtMost(availableCssWidth)
    val outerCssHeight = WINDOW_HEIGHT_CSS
    return (outerCssWidth * safeDensity).roundToInt() to (outerCssHeight * safeDensity).roundToInt()
}

private data class AudioRecordingPalette(
    val containerBg: String,
    val containerBorder: String,
    val text: String,
    val subtext: String,
    val buttonBg: String,
    val buttonHoverBg: String,
    val button: String,
    val textShadow: String,
)

private data class AudioRecordingCopy(
    val recording: String,
    val processing: String,
    val warmup: String,
    val initializing: String,
    val paused: String,
    val subtext: String,
)

private fun audioLocale(language: String): AudioRecordingCopy {
    return when (language) {
        "vi" -> AudioRecordingCopy(
            recording = "Đang ghi âm...",
            processing = "Đang xử lý...",
            warmup = "Chuẩn bị...",
            initializing = "Đang kết nối...",
            paused = "Đã tạm dừng",
            subtext = "Nhấn ESC/Hotkey để dừng",
        )

        "ko" -> AudioRecordingCopy(
            recording = "녹음 중...",
            processing = "처리 중...",
            warmup = "준비 중...",
            initializing = "연결 중...",
            paused = "일시 중지됨",
            subtext = "ESC/Hotkey를 눌러 중지",
        )

        else -> AudioRecordingCopy(
            recording = "Recording...",
            processing = "Processing...",
            warmup = "Starting...",
            initializing = "Connecting...",
            paused = "Paused",
            subtext = "Press ESC/Hotkey to stop",
        )
    }
}

private fun audioRecordingMobileShim(): String {
    return """
        (() => {
            const container = document.getElementById('container');
            if (!container) return;
            const blockInteractive = target => !!target.closest('.btn');
            let dragTouch = null;
            const TOUCH_DRAG_GAIN = Math.max(window.devicePixelRatio || 1, 1.85);

            function post(message) {
                if (window.ipc && window.ipc.postMessage) {
                    window.ipc.postMessage(message);
                }
            }

            container.addEventListener('touchstart', event => {
                if (event.touches.length !== 1 || blockInteractive(event.target)) return;
                const touch = event.touches[0];
                dragTouch = { x: touch.screenX, y: touch.screenY };
                event.preventDefault();
            }, { passive: false });

            container.addEventListener('touchmove', event => {
                if (!dragTouch || event.touches.length !== 1) return;
                const touch = event.touches[0];
                const dx = Math.round((touch.screenX - dragTouch.x) * TOUCH_DRAG_GAIN);
                const dy = Math.round((touch.screenY - dragTouch.y) * TOUCH_DRAG_GAIN);
                dragTouch = { x: touch.screenX, y: touch.screenY };
                if (dx !== 0 || dy !== 0) {
                    post(JSON.stringify({ type: 'dragAudioWindow', dx, dy }));
                }
                event.preventDefault();
            }, { passive: false });

            const finishDrag = () => {
                dragTouch = null;
            };
            container.addEventListener('touchend', finishDrag, { passive: true });
            container.addEventListener('touchcancel', finishDrag, { passive: true });
        })();
    """.trimIndent()
}

private const val BASE_ASPECT_RATIO = 16.0 / 9.0
private const val WINDOW_CHROME_INSET_CSS = 20f
private const val WINDOW_HEIGHT_CSS = 70f
private const val WINDOW_SIDE_MARGIN_CSS = 12f
private const val MIN_WINDOW_WIDTH_CSS = 300f
