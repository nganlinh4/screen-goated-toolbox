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
import android.view.View
import android.view.WindowManager
import android.widget.FrameLayout
import dev.screengoated.toolbox.mobile.MainActivity
import dev.screengoated.toolbox.mobile.model.AndroidLiveSessionRepository
import dev.screengoated.toolbox.mobile.model.MobileThemeMode
import dev.screengoated.toolbox.mobile.model.RealtimeTtsSettings
import dev.screengoated.toolbox.mobile.service.overlay.OverlayLanguagePicker
import dev.screengoated.toolbox.mobile.service.overlay.OverlayPaneWindow
import dev.screengoated.toolbox.mobile.service.overlay.RealtimeOverlayHtmlBuilder
import dev.screengoated.toolbox.mobile.service.overlay.RealtimeOverlayPaneSettings
import dev.screengoated.toolbox.mobile.service.tts.TtsConsumer
import dev.screengoated.toolbox.mobile.service.tts.TtsRuntimeService
import dev.screengoated.toolbox.mobile.service.tts.TtsRuntimeState
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionPatch
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch
import kotlin.math.roundToInt

class OverlayController(
    private val context: Context,
    private val repository: AndroidLiveSessionRepository,
    private val overlaySupported: Boolean,
    private val stopRequested: () -> Unit,
    private val restartRequested: () -> Unit,
    private val sourceModeChanged: (SourceMode) -> Unit,
    private val stopTextToSpeech: () -> Unit,
    private val ttsRuntimeService: TtsRuntimeService,
) {
    private val windowManager = context.getSystemService(android.view.WindowManager::class.java)
    private val clipboardManager = context.getSystemService(ClipboardManager::class.java)
    private val prefs = context.getSharedPreferences("sgt_overlay_window", Context.MODE_PRIVATE)
    private val htmlBuilder = RealtimeOverlayHtmlBuilder(context)
    private val languagePicker = OverlayLanguagePicker(
        context = context,
        windowManager = windowManager,
        screenBoundsProvider = ::screenBounds,
        onSelected = ::updateTargetLanguage,
    )

    private var transcriptionWindow: OverlayPaneWindow? = null
    private var translationWindow: OverlayPaneWindow? = null
    private var updateJob: Job? = null
    private var ttsRuntimeJob: Job? = null
    private var listeningVisible = true
    private var translationVisible = true
    private var lastSnapshot: OverlaySnapshot? = null
    private var lastRenderAtMs: Long = 0L
    private var renderBurstCount = 0
    private var lastSyncedVisibility: Pair<Boolean, Boolean>? = null
    private var lastTtsState: OverlayTtsState? = null
    private var lastRuntimeTtsState: TtsRuntimeState = TtsRuntimeState()

    fun show(scope: CoroutineScope): Boolean {
        if (!overlaySupported || !Settings.canDrawOverlays(context) || transcriptionWindow != null || translationWindow != null) {
            return false
        }

        listeningVisible = true
        translationVisible = true
        transcriptionWindow = createPaneWindow(OverlayPaneId.TRANSCRIPTION)
        translationWindow = createPaneWindow(OverlayPaneId.TRANSLATION)
        updatePaneVisibility()

        updateJob = scope.launch(Dispatchers.Main.immediate) {
            combine(
                repository.state,
                repository.paneFontSizes,
                repository.realtimeTtsSettings,
                repository.uiPreferences,
            ) { state, fontSizes, ttsSettings, uiPreferences ->
                OverlaySnapshot(state, fontSizes, ttsSettings, uiPreferences)
            }.collectLatest { snapshot ->
                lastSnapshot = snapshot
                render(snapshot)
            }
        }
        ttsRuntimeJob = scope.launch(Dispatchers.Main.immediate) {
            ttsRuntimeService.runtimeState.collectLatest { runtimeState ->
                lastRuntimeTtsState = runtimeState
                lastSnapshot?.let { snapshot ->
                    syncTranslationControls(snapshot, force = false)
                }
            }
        }
        return true
    }

    fun hide() {
        updateJob?.cancel()
        updateJob = null
        ttsRuntimeJob?.cancel()
        ttsRuntimeJob = null
        languagePicker.hide()
        hideDismissZone()
        lastSyncedVisibility = null
        lastTtsState = null
        lastRuntimeTtsState = TtsRuntimeState()
        transcriptionWindow?.destroy()
        translationWindow?.destroy()
        transcriptionWindow = null
        translationWindow = null
        repository.setOverlayVisible(false)
    }

    fun updateVolume(rms: Float) {
        transcriptionWindow?.evaluate("if(window.updateVolume) window.updateVolume(${rms.coerceIn(0f, 1f)});")
    }

    fun isTranslationVisible(): Boolean = translationVisible

    fun showDownloadModal() {
        transcriptionWindow?.evaluate(
            "if(window.showDownloadModal) window.showDownloadModal('Downloading Parakeet', 'Preparing...', 0);",
        )
    }

    fun hideDownloadModal() {
        transcriptionWindow?.evaluate("if(window.hideDownloadModal) window.hideDownloadModal();")
    }

    fun updateDownloadProgress(progress: Float, filename: String) {
        val msg = "Downloading $filename"
        val pct = progress.coerceIn(0f, 100f)
        transcriptionWindow?.evaluate(
            "if(window.showDownloadModal) window.showDownloadModal('Downloading Parakeet', '$msg', $pct);",
        )
    }

    private fun createPaneWindow(paneId: OverlayPaneId): OverlayPaneWindow {
        return OverlayPaneWindow(
            context = context,
            windowManager = windowManager,
            paneId = paneId,
            initialBounds = loadBounds(paneId),
            minWidthPx = OVERLAY_MIN_WIDTH_PX,
            minHeightPx = OVERLAY_MIN_HEIGHT_PX,
            screenBoundsProvider = ::screenBounds,
            onBoundsChanged = ::saveBounds,
            onMessage = ::handleBridgeMessage,
        )
    }

    private fun render(snapshot: OverlaySnapshot) {
        val now = android.os.SystemClock.elapsedRealtime()
        if (lastRenderAtMs != 0L && now - lastRenderAtMs <= 20L) {
            renderBurstCount += 1
        } else {
            renderBurstCount = 1
        }
        lastRenderAtMs = now
        if (renderBurstCount >= 3) {
            Log.d(
                PERF_TAG,
                "overlay-render-burst count=$renderBurstCount phase=${snapshot.state.phase} transcriptLen=${snapshot.state.liveText.fullTranscript.length} uncommittedLen=${snapshot.state.liveText.uncommittedTranslation.length}",
            )
        }
        val state = snapshot.state
        val transcriptionReloaded = transcriptionWindow?.render(
            html = htmlBuilder.build(
                RealtimeOverlayPaneSettings(
                    isTranslation = false,
                    isDark = isDarkTheme(snapshot.uiPreferences.themeMode),
                ),
            ),
            settings = overlayPaneRuntimeSettings(
                state = state,
                fontSize = snapshot.fontSizes.transcriptionSp,
                isDark = isDarkTheme(snapshot.uiPreferences.themeMode),
                uiLanguage = snapshot.uiPreferences.uiLanguage,
            ),
            oldText = transcriptOldText(state),
            newText = transcriptNewText(state),
        ) == true
        val translationReloaded = translationWindow?.render(
            html = htmlBuilder.build(
                RealtimeOverlayPaneSettings(
                    isTranslation = true,
                    isDark = isDarkTheme(snapshot.uiPreferences.themeMode),
                ),
            ),
            settings = overlayPaneRuntimeSettings(
                state = state,
                fontSize = snapshot.fontSizes.translationSp,
                isDark = isDarkTheme(snapshot.uiPreferences.themeMode),
                uiLanguage = snapshot.uiPreferences.uiLanguage,
            ),
            oldText = state.liveText.committedTranslation,
            newText = state.liveText.uncommittedTranslation,
        ) == true
        syncVisibility(force = transcriptionReloaded || translationReloaded)
        syncTranslationControls(snapshot, force = translationReloaded)
    }

    private fun handleBridgeMessage(
        paneId: OverlayPaneId,
        message: String,
    ) {
        when {
            message.startsWith("dragWindow:") -> {
                parseDelta(message.removePrefix("dragWindow:")) { dx, dy ->
                    windowFor(paneId)?.moveBy(
                        (dx * DRAG_WINDOW_GAIN).roundToInt(),
                        (dy * DRAG_WINDOW_GAIN).roundToInt(),
                    )
                }
                ensureDismissBubble()
            }

            message.startsWith("dragAt:") -> {
                val proximity = fingerBubbleProximity(message.removePrefix("dragAt:"))
                updateDismissZone(proximity)
            }

            message.startsWith("dragEnd:") -> {
                val proximity = fingerBubbleProximity(message.removePrefix("dragEnd:"))
                lastFingerDistSq = Int.MAX_VALUE
                if (proximity >= 0.8f) {
                    dismissOverlay(paneId)
                } else {
                    hideDismissZone()
                }
            }

            message.startsWith("resizeCorner:") -> {
                val parts = message.removePrefix("resizeCorner:").split(",")
                if (parts.size == 3) {
                    val corner = parts[0]
                    val dx = (parts[1].toIntOrNull() ?: 0) * DRAG_WINDOW_GAIN
                    val dy = (parts[2].toIntOrNull() ?: 0) * DRAG_WINDOW_GAIN
                    windowFor(paneId)?.resizeFromCorner(corner, dx.roundToInt(), dy.roundToInt())
                }
            }

            message.startsWith("copyText:") -> copyText("Realtime", message.removePrefix("copyText:"))
            message.startsWith("toggleMic:") -> toggleListening(message.removePrefix("toggleMic:") == "1")
            message.startsWith("toggleTrans:") -> toggleTranslation(message.removePrefix("toggleTrans:") == "1")
            message.startsWith("fontSize:") -> updatePaneFont(paneId, message.removePrefix("fontSize:"))
            message.startsWith("audioSource:") -> updateAudioSource(message.removePrefix("audioSource:"))
            message.startsWith("language:") -> updateTargetLanguage(message.removePrefix("language:"))
            message == "showLanguagePicker" -> showLanguagePicker()
            message.startsWith("translationModel:") -> repository.updateTranslationModel(
                message.removePrefix("translationModel:"),
            )
            message.startsWith("perf:") -> Log.d(PERF_TAG, "pane=$paneId ${message.removePrefix("perf:")}")
            message == "overlayReady" -> {
                windowFor(paneId)?.onReady()
                syncVisibility(force = true)
            }

            message.startsWith("transcriptionModel:") -> updateTranscriptionModel(
                message.removePrefix("transcriptionModel:"),
            )

            message.startsWith("ttsEnabled:") -> updateTtsEnabled(message.removePrefix("ttsEnabled:") == "1")
            message.startsWith("ttsSpeed:") -> updateTtsSpeed(message.removePrefix("ttsSpeed:"))
            message.startsWith("ttsAutoSpeed:") -> updateTtsAutoSpeed(message.removePrefix("ttsAutoSpeed:") == "1")
            message.startsWith("ttsVolume:") -> updateTtsVolume(message.removePrefix("ttsVolume:"))
            message == "cancelDownload" -> translationWindow?.evaluate(
                "if(window.hideDownloadModal) window.hideDownloadModal();",
            )
        }
    }

    private fun toggleListening(visible: Boolean) {
        listeningVisible = visible
        if (!listeningVisible && !translationVisible) {
            stopTextToSpeech()
            stopRequested()
            return
        }
        updatePaneVisibility()
        syncVisibility(force = true)
    }

    private fun toggleTranslation(visible: Boolean) {
        translationVisible = visible
        if (!translationVisible) {
            stopTextToSpeech()
            languagePicker.hide()
        }
        if (!listeningVisible && !translationVisible) {
            stopRequested()
            return
        }
        updatePaneVisibility()
        syncVisibility(force = true)
    }

    private fun syncVisibility(force: Boolean = false) {
        val next = listeningVisible to translationVisible
        if (!force && lastSyncedVisibility == next) {
            return
        }
        lastSyncedVisibility = next
        transcriptionWindow?.evaluate("if(window.setVisibility) window.setVisibility($listeningVisible, $translationVisible);")
        translationWindow?.evaluate("if(window.setVisibility) window.setVisibility($listeningVisible, $translationVisible);")
    }

    private fun updatePaneVisibility() {
        if (listeningVisible) {
            transcriptionWindow?.show()
        } else {
            transcriptionWindow?.hide()
        }
        if (translationVisible) {
            translationWindow?.show()
        } else {
            translationWindow?.hide()
        }
        repository.setOverlayVisible(listeningVisible || translationVisible)
    }

    private fun syncTranslationControls(
        snapshot: OverlaySnapshot,
        force: Boolean,
    ) {
        val ttsState = overlayTtsState(snapshot.ttsSettings, lastRuntimeTtsState)
        if (force || lastTtsState != ttsState) {
            lastTtsState = ttsState
            translationWindow?.evaluate(
                "if(window.setTtsState) window.setTtsState(${ttsState.enabled}, ${ttsState.speedPercent}, ${ttsState.autoSpeed}, ${ttsState.volumePercent});",
            )
        }
    }

    private fun updatePaneFont(
        paneId: OverlayPaneId,
        rawValue: String,
    ) {
        val size = rawValue.toIntOrNull()?.coerceIn(10, 32) ?: return
        val current = repository.currentPaneFontSizes()
        val next = when (paneId) {
            OverlayPaneId.TRANSCRIPTION -> current.copy(transcriptionSp = size)
            OverlayPaneId.TRANSLATION -> current.copy(translationSp = size)
        }
        repository.updatePaneFontSizes(next)
    }

    private fun updateAudioSource(rawValue: String) {
        val sourceMode = if (rawValue == "device") SourceMode.DEVICE else SourceMode.MIC
        val previous = repository.currentConfig().sourceMode
        repository.updateConfig(LiveSessionPatch(sourceMode = sourceMode))
        val needsProjectionConsent = sourceMode == SourceMode.DEVICE &&
            !repository.state.value.permissions.mediaProjectionGranted
        if (needsProjectionConsent) {
            repository.markAwaitingPermissions()
            launchActivityStartFlow()
            stopRequested()
            return
        }
        if (previous != sourceMode) {
            sourceModeChanged(sourceMode)
            restartRequested()
        }
    }

    private fun launchActivityStartFlow() {
        context.startActivity(
            Intent(context, MainActivity::class.java).apply {
                addFlags(
                    Intent.FLAG_ACTIVITY_NEW_TASK or
                        Intent.FLAG_ACTIVITY_SINGLE_TOP or
                        Intent.FLAG_ACTIVITY_CLEAR_TOP,
                )
                putExtra(MainActivity.EXTRA_AUTO_START, true)
            },
        )
    }

    private fun updateTargetLanguage(language: String) {
        if (language.isBlank()) {
            return
        }
        repository.updateConfig(LiveSessionPatch(targetLanguage = language))
        languagePicker.hide()
    }

    private fun showLanguagePicker() {
        val anchor = translationWindow?.currentBounds() ?: return
        val locale = dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText.forLanguage(
            repository.currentUiPreferences().uiLanguage,
        )
        languagePicker.show(
            anchorBounds = anchor,
            selectedLanguage = repository.currentConfig().targetLanguage,
            languages = repository.supportedLanguages,
            isDark = isDarkTheme(repository.currentUiPreferences().themeMode),
            title = locale.overlay.targetLanguageTitle,
        )
    }

    private fun updateTranscriptionModel(modelId: String) {
        if (repository.transcriptionModelId() != modelId) {
            repository.updateTranscriptionModel(modelId)
            restartRequested()
        }
    }

    private fun updateTtsEnabled(enabled: Boolean) {
        val current = repository.currentRealtimeTtsSettings()
        repository.updateRealtimeTtsSettings(current.copy(enabled = enabled))
        if (!enabled) {
            stopTextToSpeech()
            translationWindow?.evaluate("if(window.closeTtsModal) window.closeTtsModal();")
        }
    }

    private fun updateTtsSpeed(rawValue: String) {
        val speed = rawValue.toIntOrNull() ?: return
        repository.updateRealtimeTtsSettings(
            repository.currentRealtimeTtsSettings().copy(speedPercent = speed, autoSpeed = false),
        )
    }

    private fun updateTtsAutoSpeed(enabled: Boolean) {
        repository.updateRealtimeTtsSettings(
            repository.currentRealtimeTtsSettings().copy(autoSpeed = enabled),
        )
    }

    private fun updateTtsVolume(rawValue: String) {
        val volume = rawValue.toIntOrNull() ?: return
        repository.updateRealtimeTtsSettings(
            repository.currentRealtimeTtsSettings().copy(volumePercent = volume),
        )
    }

    private fun parseDelta(
        payload: String,
        handler: (Int, Int) -> Unit,
    ) {
        val (dx, dy) = payload.split(',', limit = 2).takeIf { it.size == 2 } ?: return
        handler(dx.toIntOrNull() ?: return, dy.toIntOrNull() ?: return)
    }

    private fun copyText(
        label: String,
        text: String,
    ) {
        val payload = text.trim()
        if (payload.isEmpty()) {
            return
        }
        clipboardManager?.setPrimaryClip(ClipData.newPlainText(label, payload))
    }

    private fun windowFor(paneId: OverlayPaneId): OverlayPaneWindow? {
        return when (paneId) {
            OverlayPaneId.TRANSCRIPTION -> transcriptionWindow
            OverlayPaneId.TRANSLATION -> translationWindow
        }
    }

    private fun loadBounds(paneId: OverlayPaneId): OverlayBounds {
        val defaults = defaultBounds(paneId)
        val screen = screenBounds()
        val width = prefs.getInt(keyFor(paneId, "width"), defaults.width).coerceIn(OVERLAY_MIN_WIDTH_PX, screen.width())
        val height = prefs.getInt(keyFor(paneId, "height"), defaults.height).coerceIn(OVERLAY_MIN_HEIGHT_PX, screen.height())
        val x = prefs.getInt(keyFor(paneId, "x"), defaults.x).coerceIn(0, (screen.width() - width).coerceAtLeast(0))
        val y = prefs.getInt(keyFor(paneId, "y"), defaults.y).coerceIn(0, (screen.height() - height).coerceAtLeast(0))
        return OverlayBounds(x = x, y = y, width = width, height = height)
    }

    private fun saveBounds(
        paneId: OverlayPaneId,
        bounds: OverlayBounds,
    ) {
        prefs.edit()
            .putInt(keyFor(paneId, "x"), bounds.x)
            .putInt(keyFor(paneId, "y"), bounds.y)
            .putInt(keyFor(paneId, "width"), bounds.width)
            .putInt(keyFor(paneId, "height"), bounds.height)
            .apply()
    }

    private fun defaultBounds(paneId: OverlayPaneId): OverlayBounds {
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

    private fun keyFor(
        paneId: OverlayPaneId,
        suffix: String,
    ): String {
        val prefix = when (paneId) {
            OverlayPaneId.TRANSCRIPTION -> "transcription_overlay"
            OverlayPaneId.TRANSLATION -> "translation_overlay"
        }
        return "${prefix}_$suffix"
    }

    private fun screenBounds(): Rect {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            windowManager.currentWindowMetrics.bounds
        } else {
            val metrics = context.resources.displayMetrics
            Rect(0, 0, metrics.widthPixels, metrics.heightPixels)
        }
    }

    private fun isDarkTheme(themeMode: MobileThemeMode): Boolean {
        return when (themeMode) {
            MobileThemeMode.DARK -> true
            MobileThemeMode.LIGHT -> false
            MobileThemeMode.SYSTEM -> {
                val nightModeFlags = context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK
                nightModeFlags == Configuration.UI_MODE_NIGHT_YES
            }
        }
    }

    // --- Dismiss bubble (drag-to-bottom-to-dismiss, mimics Android Bubbles) ---
    private var dismissBubbleView: View? = null
    private var dismissBubbleActive = false

    private fun ensureDismissBubble() {
        if (dismissBubbleView != null) return
        val bubbleSize = dp(56)
        val circle = View(context).apply {
            background = android.graphics.drawable.GradientDrawable().apply {
                shape = android.graphics.drawable.GradientDrawable.OVAL
                setColor(android.graphics.Color.argb(200, 60, 60, 60))
            }
            alpha = 0f
            scaleX = 0.4f
            scaleY = 0.4f
        }
        val icon = android.widget.TextView(context).apply {
            text = "\u00D7"
            textSize = 24f
            setTextColor(android.graphics.Color.WHITE)
            gravity = android.view.Gravity.CENTER
        }
        val container = FrameLayout(context).apply {
            addView(circle, FrameLayout.LayoutParams(bubbleSize, bubbleSize).apply {
                gravity = android.view.Gravity.CENTER
            })
            addView(icon, FrameLayout.LayoutParams(bubbleSize, bubbleSize).apply {
                gravity = android.view.Gravity.CENTER
            })
        }
        val params = WindowManager.LayoutParams(
            bubbleSize * 2,
            bubbleSize * 2,
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
                WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE or
                WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL,
            android.graphics.PixelFormat.TRANSLUCENT,
        ).apply {
            gravity = android.view.Gravity.BOTTOM or android.view.Gravity.CENTER_HORIZONTAL
            y = dp(24)
        }
        dismissBubbleView = container
        runCatching { windowManager.addView(container, params) }
        circle.animate()
            .alpha(1f).scaleX(1f).scaleY(1f)
            .setDuration(250)
            .setInterpolator(android.view.animation.OvershootInterpolator(1.5f))
            .start()
    }

    /** @param proximity 0.0 = far, 1.0 = on bubble */
    private fun updateDismissZone(proximity: Float) {
        ensureDismissBubble()
        val circle = (dismissBubbleView as? FrameLayout)?.getChildAt(0) ?: return
        // Smooth scale: 1.0 at rest → 1.35 at full proximity
        val scale = 1f + proximity * 0.35f
        circle.scaleX = scale
        circle.scaleY = scale
        // Color: grey(60,60,60) → red(220,50,50) blended by proximity
        val r = (60 + (160 * proximity)).toInt().coerceIn(0, 255)
        val g = (60 - (10 * proximity)).toInt().coerceIn(0, 255)
        val b = (60 - (10 * proximity)).toInt().coerceIn(0, 255)
        val a = (200 + (20 * proximity)).toInt().coerceIn(0, 255)
        (circle.background as? android.graphics.drawable.GradientDrawable)
            ?.setColor(android.graphics.Color.argb(a, r, g, b))
    }

    private fun hideDismissZone() {
        val view = dismissBubbleView ?: return
        val circle = (view as? FrameLayout)?.getChildAt(0)
        if (circle != null) {
            circle.animate()
                .alpha(0f)
                .scaleX(0.3f)
                .scaleY(0.3f)
                .setDuration(200)
                .withEndAction {
                    runCatching { windowManager.removeView(view) }
                    dismissBubbleView = null
                    dismissBubbleActive = false
                }
                .start()
        } else {
            runCatching { windowManager.removeView(view) }
            dismissBubbleView = null
            dismissBubbleActive = false
        }
    }

    private var lastFingerDx = 0
    private var lastFingerDy = 0
    private var lastFingerDistSq = Int.MAX_VALUE

    /**
     * Returns 0.0 (far) to 1.0 (on bubble). Uses distance + approach prediction.
     */
    private fun fingerBubbleProximity(rawXY: String): Float {
        val parts = rawXY.split(",")
        if (parts.size != 2) return 0f
        val fingerCssX = parts[0].toIntOrNull() ?: return 0f
        val fingerCssY = parts[1].toIntOrNull() ?: return 0f
        val density = context.resources.displayMetrics.density
        val screen = screenBounds()
        val bubbleCenterCssX = (screen.width() / density / 2).toInt()
        val statusBarPx = statusBarHeight()
        val bubbleCenterCssY = ((screen.height() - statusBarPx - dp(24) - dp(28)) / density).toInt()
        val dx = fingerCssX - bubbleCenterCssX
        val dy = fingerCssY - bubbleCenterCssY
        val distSq = dx * dx + dy * dy

        // Predict: if finger is moving toward bubble, activate earlier
        val approaching = distSq < lastFingerDistSq
        lastFingerDx = dx
        lastFingerDy = dy
        lastFingerDistSq = distSq

        val hitRadius = 55f // CSS px — on the bubble
        val outerRadius = if (approaching) 140f else 110f // start reacting earlier when approaching

        val dist = kotlin.math.sqrt(distSq.toFloat())
        return if (dist <= hitRadius) {
            1f
        } else if (dist <= outerRadius) {
            1f - (dist - hitRadius) / (outerRadius - hitRadius)
        } else {
            0f
        }
    }

    private fun statusBarHeight(): Int {
        val resourceId = context.resources.getIdentifier("status_bar_height", "dimen", "android")
        return if (resourceId > 0) context.resources.getDimensionPixelSize(resourceId) else dp(24)
    }

    private fun dismissOverlay(paneId: OverlayPaneId) {
        hideDismissZone()
        when (paneId) {
            OverlayPaneId.TRANSCRIPTION -> toggleListening(false)
            OverlayPaneId.TRANSLATION -> toggleTranslation(false)
        }
    }

    private fun dp(value: Int): Int {
        return (value * context.resources.displayMetrics.density).toInt()
    }

    private companion object {
        private const val OVERLAY_MIN_WIDTH_PX = 420
        private const val OVERLAY_MIN_HEIGHT_PX = 180
        private const val DRAG_WINDOW_GAIN = 1.8f
        private const val DISMISS_ZONE_PX = 120
        private const val PERF_TAG = "SGTOverlayPerf"
    }
}

internal data class OverlayTtsState(
    val enabled: Boolean,
    val speedPercent: Int,
    val autoSpeed: Boolean,
    val volumePercent: Int,
)

internal fun overlayTtsState(
    settings: RealtimeTtsSettings,
    runtimeState: TtsRuntimeState,
): OverlayTtsState {
    val displayedSpeed = if (
        settings.enabled &&
        runtimeState.isPlaying &&
        runtimeState.activeConsumer == TtsConsumer.REALTIME
    ) {
        runtimeState.currentRealtimeEffectiveSpeed.coerceIn(50, 200)
    } else {
        settings.speedPercent
    }
    return OverlayTtsState(
        enabled = settings.enabled,
        speedPercent = displayedSpeed,
        autoSpeed = settings.autoSpeed,
        volumePercent = settings.volumePercent,
    )
}
