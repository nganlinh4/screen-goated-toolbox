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
import android.widget.Toast
import dev.screengoated.toolbox.mobile.MainActivity
import dev.screengoated.toolbox.mobile.model.AndroidLiveSessionRepository
import dev.screengoated.toolbox.mobile.model.MobileThemeMode
import dev.screengoated.toolbox.mobile.model.RealtimeModelIds
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
            message.startsWith("dragWindow:") -> parseDelta(message.removePrefix("dragWindow:")) { dx, dy ->
                windowFor(paneId)?.moveBy(
                    (dx * DRAG_WINDOW_GAIN).roundToInt(),
                    (dy * DRAG_WINDOW_GAIN).roundToInt(),
                )
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
        languagePicker.show(
            anchorBounds = anchor,
            selectedLanguage = repository.currentConfig().targetLanguage,
            languages = repository.supportedLanguages,
            isDark = isDarkTheme(repository.currentUiPreferences().themeMode),
        )
    }

    private fun updateTranscriptionModel(modelId: String) {
        if (modelId == RealtimeModelIds.TRANSCRIPTION_PARAKEET) {
            Toast.makeText(context, "Parakeet on Android is not implemented yet.", Toast.LENGTH_SHORT).show()
            lastSnapshot?.let(::render)
            return
        }
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

    private fun dp(value: Int): Int {
        return (value * context.resources.displayMetrics.density).toInt()
    }

    private companion object {
        private const val OVERLAY_MIN_WIDTH_PX = 420
        private const val OVERLAY_MIN_HEIGHT_PX = 180
        private const val DRAG_WINDOW_GAIN = 1.8f
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
