package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import android.view.WindowManager
import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
import dev.screengoated.toolbox.mobile.service.overlay.overlayFontCss
import org.json.JSONObject

/** Windows selection badge with explicit touch capture and stop controls. */
internal class PresetContinuousSelection(
    private val context: Context,
    private val windowManager: WindowManager,
    private val language: () -> String,
    private val dark: () -> Boolean,
    private val stopped: () -> Unit,
    private val capture: (ResolvedPreset) -> Unit,
) {
    private var window: PresetOverlayWindow? = null
    private var invocation = 0L
    var presetId: String? = null
        private set

    fun open(preset: ResolvedPreset) {
        close()
        val openedInvocation = invocation
        presetId = preset.preset.id
        val label = when (language()) {
            "vi" -> "Bôi đen văn bản (Liên tục)"
            "ko" -> "텍스트 선택 (연속)"
            else -> "Select text (Continuous)"
        }
        val stop = when (language()) { "vi" -> "Dừng"; "ko" -> "중지"; else -> "Stop" }
        val template = context.assets.open("preset_overlay/windows_selection_badge.html")
            .bufferedReader().use { it.readText() }
        val shim = """
            <script>
            updateTheme(${dark()}); hideImageBadge(); playEntry();
            const badge = document.getElementById('text-badge');
            badge.setAttribute('role', 'button'); badge.tabIndex = 0;
            badge.onclick = () => sgtAndroid.postMessage('capture');
            badge.onkeydown = e => { if(e.key === 'Enter' || e.key === ' ') badge.click(); };
            const stop = document.createElement('button');
            stop.textContent = ${JSONObject.quote(stop)};
            stop.className = 'badge-inner';
            stop.onclick = () => sgtAndroid.postMessage('stop');
            document.querySelector('.badges-wrapper').appendChild(stop);
            </script>
        """.trimIndent()
        val html = template.replace("{{FONT_CSS}}", overlayFontCss())
            .replace("{{TEXT}}", escapeHtml(label)).replace("</body>", "$shim</body>")
        val density = context.resources.displayMetrics.density
        val width = (300 * density).toInt().coerceAtMost(context.resources.displayMetrics.widthPixels)
        window = PresetOverlayWindow(context, windowManager,
            PresetOverlayWindowSpec(width, (112 * density).toInt(),
                (context.resources.displayMetrics.widthPixels - width) / 2, (32 * density).toInt(),
                focusable = false, htmlContent = html, clipToOutline = false),
            onMessage = { message ->
                if (invocation == openedInvocation && presetId == preset.preset.id) when (message) {
                    "capture" -> capture(preset)
                    "stop" -> { close(); stopped() }
                }
            },
        ).also { it.show() }
    }

    fun close() {
        invocation++
        presetId = null
        window?.destroy()
        window = null
    }
}
