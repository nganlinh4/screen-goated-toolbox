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

internal fun OverlayController.isNearDismissArea(bounds: OverlayBounds): Boolean {
    val screen = screenBounds()
    val dismissTop = (screen.height() - dp(DISMISS_ZONE_PX)).coerceAtLeast(0)
    return bounds.y + bounds.height >= dismissTop
}

/** Currently-visible panes, in stacking order (transcription above translation). */
internal fun OverlayController.shownPaneWindows(): List<Pair<OverlayPaneId, OverlayPaneWindow>> =
    buildList {
        transcriptionWindow?.let { if (listeningVisible) add(OverlayPaneId.TRANSCRIPTION to it) }
        translationWindow?.let { if (translationVisible) add(OverlayPaneId.TRANSLATION to it) }
    }

/** Resolve the snap mode from proximity with hysteresis so it doesn't flap at the edges. */
internal fun snapModeFor(current: DismissAction, hit: DismissHit): DismissAction {
    val single = hit.singleProximity
    val all = hit.allProximity
    val allWins = all >= single
    val enter = SNAP_ENTER_PROXIMITY
    val exit = SNAP_EXIT_PROXIMITY
    return when (current) {
        DismissAction.ALL -> when {
            single >= enter && !allWins -> DismissAction.SINGLE
            all >= exit -> DismissAction.ALL
            else -> DismissAction.NONE
        }
        DismissAction.SINGLE -> when {
            all >= enter && allWins -> DismissAction.ALL
            single >= exit -> DismissAction.SINGLE
            else -> DismissAction.NONE
        }
        DismissAction.NONE -> when {
            all >= enter && allWins -> DismissAction.ALL
            single >= enter -> DismissAction.SINGLE
            else -> DismissAction.NONE
        }
    }
}

/** Commit the drag: snapped panes swallow into the bubble and dismiss; otherwise fly home. */
internal fun OverlayController.finishDragGesture(
    paneId: OverlayPaneId,
    committed: DismissAction,
) {
    when (committed) {
        DismissAction.ALL -> {
            dismissBubble.swallow(DismissAction.ALL) {}
            dragSnap.finish(
                committed = DismissAction.ALL,
                onCommit = {
                    stopTextToSpeech()
                    stopRequested()
                },
                onSettle = {},
            )
        }
        DismissAction.SINGLE -> {
            dismissBubble.swallow(DismissAction.SINGLE) {}
            dragSnap.finish(
                committed = DismissAction.SINGLE,
                onCommit = {
                    when (paneId) {
                        OverlayPaneId.TRANSCRIPTION -> toggleListening(false)
                        OverlayPaneId.TRANSLATION -> toggleTranslation(false)
                    }
                },
                onSettle = {},
            )
        }
        DismissAction.NONE -> {
            val free = dragSnap.freeBoundsOf(paneId)
            dragSnap.finish(
                committed = DismissAction.NONE,
                onCommit = {},
                onSettle = {
                    (free ?: windowFor(paneId)?.currentBounds())?.let { saveBounds(paneId, it) }
                    dismissBubble.hide()
                },
            )
        }
    }
}

internal fun OverlayController.dp(value: Int): Int {
    return (value * context.resources.displayMetrics.density).toInt()
}


