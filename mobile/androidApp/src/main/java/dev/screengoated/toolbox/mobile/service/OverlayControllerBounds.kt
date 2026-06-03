package dev.screengoated.toolbox.mobile.service

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.res.Configuration
import android.graphics.Rect
import android.os.Build
import android.provider.Settings
import android.util.Log
import android.view.WindowManager
import androidx.core.content.edit
import dev.screengoated.toolbox.mobile.ProjectionConsentProxyActivity
import dev.screengoated.toolbox.mobile.model.AndroidLiveSessionRepository
import dev.screengoated.toolbox.mobile.model.MobileThemeMode
import dev.screengoated.toolbox.mobile.model.RealtimeModelIds
import dev.screengoated.toolbox.mobile.model.RealtimeTtsSettings
import dev.screengoated.toolbox.mobile.service.overlay.OverlayLanguagePicker
import dev.screengoated.toolbox.mobile.service.overlay.OverlayPickerOption
import dev.screengoated.toolbox.mobile.service.overlay.OverlayPaneWindow
import dev.screengoated.toolbox.mobile.service.overlay.RealtimeOverlayModelOptions
import dev.screengoated.toolbox.mobile.service.overlay.RealtimeOverlayHtmlBuilder
import dev.screengoated.toolbox.mobile.service.overlay.RealtimeOverlayPaneSettings
import dev.screengoated.toolbox.mobile.service.tts.TtsConsumer
import dev.screengoated.toolbox.mobile.service.tts.TtsRuntimeService
import dev.screengoated.toolbox.mobile.service.tts.TtsRuntimeState
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionPatch
import dev.screengoated.toolbox.mobile.shared.live.LiveTranslateParity
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch
import org.json.JSONObject
import kotlin.math.roundToInt


// Window bounds persistence + dismiss-zone handling extracted from OverlayController.
internal fun OverlayController.windowFor(paneId: OverlayPaneId): OverlayPaneWindow? {
    return when (paneId) {
        OverlayPaneId.TRANSCRIPTION -> transcriptionWindow
        OverlayPaneId.TRANSLATION -> translationWindow
    }
}

internal fun OverlayController.loadBounds(paneId: OverlayPaneId): OverlayBounds {
    val defaults = defaultBounds(paneId)
    val screen = screenBounds()
    val width = prefs.getInt(keyFor(paneId, "width"), defaults.width).coerceIn(OVERLAY_MIN_WIDTH_PX, screen.width())
    val height = prefs.getInt(keyFor(paneId, "height"), defaults.height).coerceIn(OVERLAY_MIN_HEIGHT_PX, screen.height())
    val x = prefs.getInt(keyFor(paneId, "x"), defaults.x).coerceIn(0, (screen.width() - width).coerceAtLeast(0))
    val y = prefs.getInt(keyFor(paneId, "y"), defaults.y).coerceIn(0, (screen.height() - height).coerceAtLeast(0))
    val loaded = OverlayBounds(x = x, y = y, width = width, height = height)
    return if (isNearDismissArea(loaded)) defaults else loaded
}

internal fun OverlayController.saveBounds(
    paneId: OverlayPaneId,
    bounds: OverlayBounds,
) {
    prefs.edit {
        putInt(keyFor(paneId, "x"), bounds.x)
        putInt(keyFor(paneId, "y"), bounds.y)
        putInt(keyFor(paneId, "width"), bounds.width)
        putInt(keyFor(paneId, "height"), bounds.height)
    }
}

internal fun OverlayController.defaultBounds(paneId: OverlayPaneId): OverlayBounds {
    val screen = screenBounds()
    val portrait = screen.height() > screen.width()
    val gap = dp(14)
    val width = if (portrait) {
        (screen.width() * 0.92f).toInt()
    } else {
        (screen.width() * 0.46f).toInt()
    }.coerceAtLeast(OVERLAY_MIN_WIDTH_PX)
    val height = if (portrait) {
        (screen.height() * 0.22f).toInt()
    } else {
        (screen.height() * 0.34f).toInt()
    }.coerceAtLeast(OVERLAY_MIN_HEIGHT_PX)
    return if (portrait) {
        val top = dp(68)
        val x = ((screen.width() - width) / 2).coerceAtLeast(0)
        val y = when (paneId) {
            OverlayPaneId.TRANSCRIPTION -> top
            OverlayPaneId.TRANSLATION -> (top + height + gap).coerceAtMost(screen.height() - height)
        }
        OverlayBounds(x = x, y = y, width = width.coerceAtMost(screen.width()), height = height.coerceAtMost(screen.height()))
    } else {
        val margin = dp(22)
        val x = when (paneId) {
            OverlayPaneId.TRANSCRIPTION -> margin
            OverlayPaneId.TRANSLATION -> (screen.width() - width - margin).coerceAtLeast(margin)
        }
        OverlayBounds(
            x = x.coerceIn(0, (screen.width() - width).coerceAtLeast(0)),
            y = dp(42).coerceIn(0, (screen.height() - height).coerceAtLeast(0)),
            width = width.coerceAtMost(screen.width()),
            height = height.coerceAtMost(screen.height()),
        )
    }
}

internal fun OverlayController.keyFor(
    paneId: OverlayPaneId,
    suffix: String,
): String {
    val prefix = when (paneId) {
        OverlayPaneId.TRANSCRIPTION -> "transcription_overlay"
        OverlayPaneId.TRANSLATION -> "translation_overlay"
    }
    return "${prefix}_$suffix"
}

internal fun OverlayController.screenBounds(): Rect {
    return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
        windowManager.currentWindowMetrics.bounds
    } else {
        val metrics = context.resources.displayMetrics
        Rect(0, 0, metrics.widthPixels, metrics.heightPixels)
    }
}

internal fun OverlayController.isDarkTheme(themeMode: MobileThemeMode): Boolean {
    return when (themeMode) {
        MobileThemeMode.DARK -> true
        MobileThemeMode.LIGHT -> false
        MobileThemeMode.SYSTEM -> {
            val nightModeFlags = context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK
            nightModeFlags == Configuration.UI_MODE_NIGHT_YES
        }
    }
}

internal fun OverlayController.ensureDismissBubble() {
    if (dismissZone != null) return
    dismissZone = MorphDismissZone(
        context = context,
        windowManager = windowManager,
        targets = dismissTargets,
    ).also { it.show() }
}

internal fun OverlayController.updateDismissZone(rawXY: String) {
    ensureDismissBubble()
    dismissZone?.update(floatArrayOf(dismissZoneProximity(rawXY)))
}

internal fun OverlayController.hideDismissZone() {
    dismissZone?.hide()
    dismissZone = null
    resetDismissTracking()
}

internal fun OverlayController.dismissZoneProximity(rawXY: String): Float {
    val parts = rawXY.split(",")
    if (parts.size != 2) return 0f
    val fingerCssX = parts[0].toFloatOrNull() ?: return 0f
    val fingerCssY = parts[1].toFloatOrNull() ?: return 0f
    val density = context.resources.displayMetrics.density
    val hit = MorphDismissZone.hitTest(
        rawX = fingerCssX,
        rawY = fingerCssY,
        screenBounds = screenBounds(),
        density = density,
        coordinateScale = density,
        targets = dismissTargets,
        previousDistanceSq = lastDismissDistanceSq,
        layoutDirection = context.resources.configuration.layoutDirection,
    )
    hit.distanceSq.copyInto(lastDismissDistanceSq)
    return hit.proximities.firstOrNull() ?: 0f
}

internal fun OverlayController.isNearDismissArea(bounds: OverlayBounds): Boolean {
    val screen = screenBounds()
    val dismissTop = (screen.height() - dp(DISMISS_ZONE_PX)).coerceAtLeast(0)
    return bounds.y + bounds.height >= dismissTop
}

internal fun OverlayController.resetDismissTracking() {
    lastDismissDistanceSq.fill(Float.POSITIVE_INFINITY)
}

internal fun OverlayController.dismissOverlay(paneId: OverlayPaneId) {
    hideDismissZone()
    when (paneId) {
        OverlayPaneId.TRANSCRIPTION -> toggleListening(false)
        OverlayPaneId.TRANSLATION -> toggleTranslation(false)
    }
}

internal fun OverlayController.dp(value: Int): Int {
    return (value * context.resources.displayMetrics.density).toInt()
}


