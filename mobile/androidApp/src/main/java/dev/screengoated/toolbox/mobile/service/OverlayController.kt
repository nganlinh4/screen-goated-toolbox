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

class OverlayController(
    internal val context: Context,
    internal val repository: AndroidLiveSessionRepository,
    internal val overlaySupported: Boolean,
    internal val stopRequested: () -> Unit,
    internal val cancelDownloadRequested: () -> Unit,
    internal val restartRequested: () -> Unit,
    internal val sourceModeChanged: (SourceMode) -> Unit,
    internal val stopTextToSpeech: () -> Unit,
    internal val ttsRuntimeService: TtsRuntimeService,
) {
    internal val windowManager = context.getSystemService(android.view.WindowManager::class.java)
    internal val clipboardManager = context.getSystemService(ClipboardManager::class.java)
    internal val prefs = context.getSharedPreferences("sgt_overlay_window", Context.MODE_PRIVATE)
    internal val htmlBuilder = RealtimeOverlayHtmlBuilder(context)
    internal val languagePicker = OverlayLanguagePicker(
        context = context,
        windowManager = windowManager,
        screenBoundsProvider = this::screenBounds,
        onSelected = ::updateTargetLanguage,
    )
    internal val transcriptionLanguagePicker = OverlayLanguagePicker(
        context = context,
        windowManager = windowManager,
        screenBoundsProvider = this::screenBounds,
        onSelected = this::onTranscriptionLanguageSelected,
    )

    internal var transcriptionWindow: OverlayPaneWindow? = null
    internal var translationWindow: OverlayPaneWindow? = null
    internal var updateJob: Job? = null
    internal var ttsRuntimeJob: Job? = null
    internal var listeningVisible = true
    internal var translationVisible = true
    internal var lastSnapshot: OverlaySnapshot? = null
    internal var lastRenderAtMs: Long = 0L
    internal var renderBurstCount = 0
    internal var lastSyncedVisibility: Pair<Boolean, Boolean>? = null
    internal var lastTtsState: OverlayTtsState? = null
    internal var lastRuntimeTtsState: TtsRuntimeState = TtsRuntimeState()
    internal val dismissTargets = MorphDismissZone.singleDismiss()
    internal var dismissZone: MorphDismissZone? = null
    internal val lastDismissDistanceSq = FloatArray(dismissTargets.size) { Float.POSITIVE_INFINITY }
    internal val boundsPersistenceSuspended = mutableSetOf<OverlayPaneId>()

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
        transcriptionLanguagePicker.hide()
        transcriptionModelPicker.hide()
        translationModelPicker.hide()
        hideDismissZone()
        lastSyncedVisibility = null
        lastTtsState = null
        lastRuntimeTtsState = TtsRuntimeState()
        boundsPersistenceSuspended.clear()
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

    internal var downloadModalSubject = ""

    fun showDownloadModal(title: String = "Model") {
        downloadModalSubject = title
        val overlay = currentOverlayLocale()
        val message = title.ifBlank { overlay.pleaseWaitText }
        transcriptionWindow?.evaluate(
            buildString {
                append("if(window.showDownloadModal) window.showDownloadModal(")
                append(JSONObject.quote(overlay.downloadingModelTitle))
                append(", ")
                append(JSONObject.quote(message))
                append(", 0);")
            },
        )
    }

    fun hideDownloadModal() {
        transcriptionWindow?.evaluate("if(window.hideDownloadModal) window.hideDownloadModal();")
    }

    fun updateDownloadProgress(progress: Float, filename: String) {
        val overlay = currentOverlayLocale()
        val message = filename.ifBlank {
            downloadModalSubject.ifBlank { overlay.pleaseWaitText }
        }
        val pct = progress.coerceIn(0f, 100f)
        transcriptionWindow?.evaluate(
            buildString {
                append("if(window.showDownloadModal) window.showDownloadModal(")
                append(JSONObject.quote(overlay.downloadingModelTitle))
                append(", ")
                append(JSONObject.quote(message))
                append(", ")
                append(pct)
                append(");")
            },
        )
    }

    internal fun createPaneWindow(paneId: OverlayPaneId): OverlayPaneWindow {
        return OverlayPaneWindow(
            context = context,
            windowManager = windowManager,
            paneId = paneId,
            initialBounds = loadBounds(paneId),
            minWidthPx = OVERLAY_MIN_WIDTH_PX,
            minHeightPx = OVERLAY_MIN_HEIGHT_PX,
            screenBoundsProvider = this::screenBounds,
            onBoundsChanged = { id, bounds ->
                if (id !in boundsPersistenceSuspended) {
                    saveBounds(id, bounds)
                }
            },
            onMessage = ::handleBridgeMessage,
        )
    }

    internal fun render(snapshot: OverlaySnapshot) {
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
            newText = if (state.liveText.committedTranslation.isNotBlank() && state.liveText.uncommittedTranslation.isNotBlank())
                " ${state.liveText.uncommittedTranslation}" else state.liveText.uncommittedTranslation,
        ) == true
        syncVisibility(force = transcriptionReloaded || translationReloaded)
        syncTranslationControls(snapshot, force = translationReloaded)
    }

    internal fun handleBridgeMessage(
        paneId: OverlayPaneId,
        message: String,
    ) {
        when {
            message.startsWith("dragWindow:") -> {
                boundsPersistenceSuspended.add(paneId)
                parseDelta(message.removePrefix("dragWindow:")) { dx, dy ->
                    windowFor(paneId)?.moveBy(
                        (dx * DRAG_WINDOW_GAIN).roundToInt(),
                        (dy * DRAG_WINDOW_GAIN).roundToInt(),
                    )
                }
                ensureDismissBubble()
            }

            message.startsWith("dragAt:") -> {
                val rawXY = message.removePrefix("dragAt:")
                updateDismissZone(rawXY)
            }

            message.startsWith("dragEnd:") -> {
                val proximity = dismissZoneProximity(message.removePrefix("dragEnd:"))
                resetDismissTracking()
                val currentBounds = windowFor(paneId)?.currentBounds()
                if (proximity >= DISMISS_THRESHOLD) {
                    dismissOverlay(paneId)
                } else {
                    currentBounds?.let { saveBounds(paneId, it) }
                    hideDismissZone()
                }
                boundsPersistenceSuspended.remove(paneId)
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

            message == "showTranscriptionLanguagePicker" -> showTranscriptionLanguagePicker()
            message == "showTranscriptionModelPicker" -> showTranscriptionModelPicker()
            message == "showTranslationModelPicker" -> showTranslationModelPicker()

            message.startsWith("transcriptionLanguage:") -> {
                val code = message.removePrefix("transcriptionLanguage:")
                repository.updateTranscriptionLanguage(code)
                restartRequested()
            }

            message.startsWith("ttsEnabled:") -> updateTtsEnabled(message.removePrefix("ttsEnabled:") == "1")
            message.startsWith("ttsSpeed:") -> updateTtsSpeed(message.removePrefix("ttsSpeed:"))
            message.startsWith("ttsAutoSpeed:") -> updateTtsAutoSpeed(message.removePrefix("ttsAutoSpeed:") == "1")
            message.startsWith("ttsVolume:") -> updateTtsVolume(message.removePrefix("ttsVolume:"))
            message == "cancelDownload" -> {
                hideDownloadModal()
                cancelDownloadRequested()
            }
        }
    }

    internal fun toggleListening(visible: Boolean) {
        listeningVisible = visible
        if (!listeningVisible && !translationVisible) {
            stopTextToSpeech()
            stopRequested()
            return
        }
        updatePaneVisibility()
        syncVisibility(force = true)
    }

    internal fun toggleTranslation(visible: Boolean) {
        translationVisible = visible
        if (!translationVisible) {
            stopTextToSpeech()
            languagePicker.hide()
            transcriptionLanguagePicker.hide()
            transcriptionModelPicker.hide()
            translationModelPicker.hide()
        }
        if (!listeningVisible && !translationVisible) {
            stopRequested()
            return
        }
        updatePaneVisibility()
        syncVisibility(force = true)
    }

    internal fun syncVisibility(force: Boolean = false) {
        val next = listeningVisible to translationVisible
        if (!force && lastSyncedVisibility == next) {
            return
        }
        lastSyncedVisibility = next
        transcriptionWindow?.evaluate("if(window.setVisibility) window.setVisibility($listeningVisible, $translationVisible);")
        translationWindow?.evaluate("if(window.setVisibility) window.setVisibility($listeningVisible, $translationVisible);")
    }

    internal fun updatePaneVisibility() {
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

    internal fun syncTranslationControls(
        snapshot: OverlaySnapshot,
        force: Boolean,
    ) {
        val ttsState = overlayTtsState(
            settings = snapshot.ttsSettings,
            runtimeState = lastRuntimeTtsState,
            forceEnabled = dev.screengoated.toolbox.mobile.model.RealtimeModelIds.isGeminiS2sModelId(
                snapshot.state.config.transcriptionProvider.id,
            ),
        )
        if (force || lastTtsState != ttsState) {
            lastTtsState = ttsState
            translationWindow?.evaluate(
                "if(window.setTtsState) window.setTtsState(${ttsState.enabled}, ${ttsState.speedPercent}, ${ttsState.autoSpeed}, ${ttsState.volumePercent});",
            )
        }
    }

    internal fun updatePaneFont(
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

    internal fun updateAudioSource(rawValue: String) {
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

    internal fun launchActivityStartFlow() {
        context.startActivity(ProjectionConsentProxyActivity.startSessionIntent(context))
    }

    internal fun updateTargetLanguage(language: String) {
        if (language.isBlank()) {
            return
        }
        val previousConfig = repository.currentConfig()
        repository.updateConfig(LiveSessionPatch(targetLanguage = language))
        languagePicker.hide()
        transcriptionLanguagePicker.hide()
        transcriptionModelPicker.hide()
        translationModelPicker.hide()
        if (
            LiveTranslateParity.targetLanguageChangeRequiresRestart(
                previousLanguage = previousConfig.targetLanguage,
                nextLanguage = language,
                transcriptionProviderId = previousConfig.transcriptionProvider.id,
            )
        ) {
            restartRequested()
        }
    }

    internal val transcriptionModelPicker = dev.screengoated.toolbox.mobile.service.overlay.OverlayLanguagePicker(
        context = context,
        windowManager = windowManager,
        screenBoundsProvider = this::screenBounds,
        onSelected = this::onTranscriptionModelSelected,
    )
    internal val translationModelPicker = dev.screengoated.toolbox.mobile.service.overlay.OverlayLanguagePicker(
        context = context,
        windowManager = windowManager,
        screenBoundsProvider = this::screenBounds,
        onSelected = this::onTranslationModelSelected,
    )

    internal fun updateTtsEnabled(enabled: Boolean) {
        if (RealtimeModelIds.isGeminiS2sModelId(repository.transcriptionModelId())) {
            val current = repository.currentRealtimeTtsSettings()
            repository.updateRealtimeTtsSettings(current.copy(enabled = true))
            return
        }
        val current = repository.currentRealtimeTtsSettings()
        repository.updateRealtimeTtsSettings(current.copy(enabled = enabled))
        if (!enabled) {
            stopTextToSpeech()
            translationWindow?.evaluate("if(window.closeTtsModal) window.closeTtsModal();")
        }
    }

    internal fun updateTtsSpeed(rawValue: String) {
        val speed = rawValue.toIntOrNull() ?: return
        repository.updateRealtimeTtsSettings(
            repository.currentRealtimeTtsSettings().copy(speedPercent = speed, autoSpeed = false),
        )
    }

    internal fun updateTtsAutoSpeed(enabled: Boolean) {
        repository.updateRealtimeTtsSettings(
            repository.currentRealtimeTtsSettings().copy(autoSpeed = enabled),
        )
    }

    internal fun updateTtsVolume(rawValue: String) {
        val volume = rawValue.toIntOrNull() ?: return
        repository.updateRealtimeTtsSettings(
            repository.currentRealtimeTtsSettings().copy(volumePercent = volume),
        )
    }

    internal fun parseDelta(
        payload: String,
        handler: (Int, Int) -> Unit,
    ) {
        val (dx, dy) = payload.split(',', limit = 2).takeIf { it.size == 2 } ?: return
        handler(dx.toIntOrNull() ?: return, dy.toIntOrNull() ?: return)
    }

    internal fun copyText(
        label: String,
        text: String,
    ) {
        val payload = text.trim()
        if (payload.isEmpty()) {
            return
        }
        clipboardManager?.setPrimaryClip(ClipData.newPlainText(label, payload))
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
    forceEnabled: Boolean = false,
): OverlayTtsState {
    val displayedSpeed = if (
        (settings.enabled || forceEnabled) &&
        runtimeState.isPlaying &&
        runtimeState.activeConsumer == TtsConsumer.REALTIME
    ) {
        runtimeState.currentRealtimeEffectiveSpeed.coerceIn(50, 200)
    } else {
        settings.speedPercent
    }
    return OverlayTtsState(
        enabled = settings.enabled || forceEnabled,
        speedPercent = displayedSpeed,
        autoSpeed = settings.autoSpeed,
        volumePercent = settings.volumePercent,
    )
}

// Shared overlay layout constants (promoted from the former companion object).
internal const val OVERLAY_MIN_WIDTH_PX = 420
internal const val OVERLAY_MIN_HEIGHT_PX = 180
internal const val DRAG_WINDOW_GAIN = 1f
internal const val DISMISS_THRESHOLD = 0.8f
internal const val DISMISS_ZONE_PX = 120
internal const val PERF_TAG = "SGTOverlayPerf"
