package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import android.graphics.Rect
import android.view.WindowManager
import android.widget.Toast
import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
import dev.screengoated.toolbox.mobile.service.OverlayBounds

internal class PresetOverlayPanelModule(
    private val context: Context,
    private val windowManager: WindowManager,
    private val favoriteBubbleHtmlBuilder: FavoriteBubbleHtmlBuilder,
    private val uiLanguage: () -> String,
    private val isDarkTheme: () -> Boolean,
    private val keepOpenProvider: () -> Boolean,
    private val onKeepOpenChanged: (Boolean) -> Unit,
    private val onIncreaseBubbleSize: () -> Unit,
    private val onDecreaseBubbleSize: () -> Unit,
    private val onPanelExpandedChanged: (Boolean) -> Unit,
    private val onRequestBubbleFront: () -> Unit,
    private val bubbleBoundsProvider: () -> OverlayBounds,
    private val screenBoundsProvider: () -> Rect,
    private val density: Float,
    private val cssToPhysical: (Float) -> Int,
    private val dp: (Int) -> Int,
    private val favoritePanelPresets: () -> List<ResolvedPreset>,
    private val resolvedPresetById: (String) -> ResolvedPreset?,
    private val launchPreset: (String, Boolean, Boolean) -> Unit,
) {
    private var panelWindow: PresetOverlayWindow? = null
    private var panelPresetIds: List<String> = emptyList()
    private var panelClosing = false

    fun hasWindow(): Boolean = panelWindow != null

    fun updateBubbleBounds() {
        val window = panelWindow ?: return
        val panelSpec = panelWindowSpec()
        window.updateBounds(
            OverlayBounds(
                x = panelSpec.x,
                y = panelSpec.y,
                width = panelSpec.width,
                height = window.currentBounds().height,
            ),
        )
        syncPanelWindowState(window)
    }

    fun toggle() {
        runCatching {
            if (panelWindow != null) {
                close(animate = true)
            } else {
                open()
            }
        }.onFailure {
            close(animate = false)
            Toast.makeText(
                context,
                overlayLocalized(
                    uiLanguage(),
                    "Bubble panel is not ready yet.",
                    "Bảng điều khiển bong bóng chưa sẵn sàng.",
                    "버블 패널이 아직 준비되지 않았습니다.",
                ),
                Toast.LENGTH_SHORT,
            ).show()
        }
    }

    fun dismiss() {
        close(animate = false)
    }

    fun destroy() {
        onPanelExpandedChanged(false)
        panelWindow?.destroy()
        panelWindow = null
        panelPresetIds = emptyList()
        panelClosing = false
    }

    fun refresh() {
        if (panelWindow != null) {
            render(animate = false)
        }
    }

    fun setSuppressed(suppressed: Boolean) {
        panelWindow?.setSuppressed(suppressed)
    }

    fun handleMessage(message: String) {
        when {
            message == "dismiss" || message == "close_now" -> close(animate = false)
            message == "focus_bubble" || message == "panel_ready" -> {}
            message.startsWith("resize:") -> {
                val window = panelWindow ?: return
                val measuredHeight = message.substringAfter("resize:", "").toIntOrNull() ?: return
                val clampedHeight = measuredHeight
                    .coerceAtLeast(minPanelHeight())
                    .coerceAtMost((screenBoundsProvider().height() * 0.62f).toInt())
                if (clampedHeight != window.currentBounds().height) {
                    window.updateBounds(window.currentBounds().copy(height = clampedHeight))
                }
            }
            message.startsWith("trigger:") ||
                message.startsWith("trigger_only:") ||
                message.startsWith("trigger_continuous:") ||
                message.startsWith("trigger_continuous_only:") -> {
                val index = message.substringAfter(':').toIntOrNull() ?: return
                val presetId = panelPresetIds.getOrNull(index) ?: return
                val continuous = message.startsWith("trigger_continuous:")
                    || message.startsWith("trigger_continuous_only:")
                launchPreset(
                    presetId,
                    false,
                    continuous,
                )
                if (message.startsWith("trigger_only:") || message.startsWith("trigger_continuous_only:")) {
                    // Panel doesn't overlap bubble, no z-reorder needed
                }
            }
            message.startsWith("set_keep_open:") -> {
                onKeepOpenChanged(message.substringAfter("set_keep_open:", "") == "1")
            }
            message == "increase_size" -> {
                onIncreaseBubbleSize()
            }
            message == "decrease_size" -> {
                onDecreaseBubbleSize()
            }
            message.startsWith("{") -> {
                val payload = message.jsonOrNull() ?: return
                when (payload.optString("type")) {
                    "closePanel" -> close(animate = true)
                    "launchPreset" -> launchPreset(payload.optString("presetId"), true, false)
                    "panelRendered" -> Unit
                    "showUnsupported" -> {
                        val presetId = payload.optString("presetId")
                        val reason = resolvedPresetById(presetId)?.executionCapability?.reason
                        if (reason != null) {
                            Toast.makeText(context, placeholderReasonLabel(reason, uiLanguage()), Toast.LENGTH_SHORT).show()
                        }
                    }
                }
            }
        }
    }

    private fun open() {
        val favorites = favoritePanelPresets()
        if (favorites.isEmpty()) {
            Toast.makeText(context, emptyFavoritesMessage(uiLanguage()), Toast.LENGTH_SHORT).show()
            return
        }
        panelClosing = false
        val spec = panelWindowSpec()
        panelWindow = PresetOverlayWindow(
            context = context,
            windowManager = windowManager,
            spec = spec.copy(
                htmlContent = buildPanelHtml(favorites),
                baseUrl = FAVORITE_PANEL_BASE_URL,
                clipToOutline = false,
            ),
            onMessage = ::handleMessage,
        ).also { window ->
            window.show()
            onPanelExpandedChanged(true)
            syncPanelWindowState(window)
            window.runScript(openPanelScriptSupport(window.currentBounds(), bubbleBoundsProvider(), density))
            onRequestBubbleFront()
        }
    }

    private fun render(animate: Boolean) {
        val window = panelWindow ?: return
        val favorites = favoritePanelPresets()
        if (favorites.isEmpty()) {
            close(animate = false)
            Toast.makeText(context, emptyFavoritesMessage(uiLanguage()), Toast.LENGTH_SHORT).show()
            return
        }
        val panelSpec = panelWindowSpec()
        window.updateBounds(
            OverlayBounds(
                x = panelSpec.x,
                y = panelSpec.y,
                width = panelSpec.width,
                height = window.currentBounds().height.coerceAtLeast(minPanelHeight()),
            ),
        )
        window.loadHtmlContent(buildPanelHtml(favorites), FAVORITE_PANEL_BASE_URL)
        syncPanelWindowState(window)
        window.runScript(
            if (animate) {
                openPanelScriptSupport(window.currentBounds(), bubbleBoundsProvider(), density)
            } else {
                showPanelImmediatelyScriptSupport()
            },
        )
    }

    private fun close(animate: Boolean) {
        val window = panelWindow ?: return
        if (!animate) {
            panelClosing = false
            panelPresetIds = emptyList()
            onPanelExpandedChanged(false)
            window.destroy()
            panelWindow = null
            return
        }
        if (panelClosing) {
            return
        }
        panelClosing = true
        syncPanelWindowState(window)
        window.runScript("window.closePanel();")
    }

    private fun buildPanelHtml(favorites: List<ResolvedPreset>): String {
        val build = buildPanelHtmlSupport(
            builder = favoriteBubbleHtmlBuilder,
            favorites = favorites,
            uiLanguage = uiLanguage(),
            isDark = isDarkTheme(),
            keepOpenEnabled = keepOpenProvider(),
            columnCount = panelColumnCountSupport(
                itemCount = favorites.size,
                bubbleBounds = bubbleBoundsProvider(),
                density = density,
                screenBounds = screenBoundsProvider(),
            ),
        )
        panelPresetIds = build.presetIds
        return build.html
    }

    private fun panelWindowSpec(): PresetOverlayWindowSpec {
        return panelWindowSpecSupport(
            itemCount = favoritePanelPresets().size,
            bubbleBounds = bubbleBoundsProvider(),
            density = density,
            screenBounds = screenBoundsProvider(),
            cssToPhysical = cssToPhysical,
        )
    }

    private fun minPanelHeight(): Int {
        return minPanelHeightSupport(
            itemCount = panelPresetIds.size.coerceAtLeast(favoritePanelPresets().size),
            bubbleBounds = bubbleBoundsProvider(),
            density = density,
            screenBounds = screenBoundsProvider(),
            cssToPhysical = cssToPhysical,
        )
    }

    private fun syncPanelWindowState(window: PresetOverlayWindow) {
        window.runScript(
            syncPanelWindowStateScriptSupport(
                panelBounds = window.currentBounds(),
                bubbleBounds = bubbleBoundsProvider(),
                density = density,
                screenBounds = screenBoundsProvider(),
            ),
        )
    }
}
