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
import org.json.JSONObject
import kotlin.math.roundToInt

class OverlayController(
    private val context: Context,
    private val repository: AndroidLiveSessionRepository,
    private val overlaySupported: Boolean,
    private val stopRequested: () -> Unit,
    private val cancelDownloadRequested: () -> Unit,
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
    private val transcriptionLanguagePicker = OverlayLanguagePicker(
        context = context,
        windowManager = windowManager,
        screenBoundsProvider = ::screenBounds,
        onSelected = ::onTranscriptionLanguageSelected,
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
    private val dismissTargets = MorphDismissZone.singleDismiss()
    private var dismissZone: MorphDismissZone? = null
    private val lastDismissDistanceSq = FloatArray(dismissTargets.size) { Float.POSITIVE_INFINITY }
    private val boundsPersistenceSuspended = mutableSetOf<OverlayPaneId>()

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

    private var downloadModalSubject = ""

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

    private fun createPaneWindow(paneId: OverlayPaneId): OverlayPaneWindow {
        return OverlayPaneWindow(
            context = context,
            windowManager = windowManager,
            paneId = paneId,
            initialBounds = loadBounds(paneId),
            minWidthPx = OVERLAY_MIN_WIDTH_PX,
            minHeightPx = OVERLAY_MIN_HEIGHT_PX,
            screenBoundsProvider = ::screenBounds,
            onBoundsChanged = { id, bounds ->
                if (id !in boundsPersistenceSuspended) {
                    saveBounds(id, bounds)
                }
            },
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
                    uiLanguage = snapshot.uiPreferences.uiLanguage,
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
                    uiLanguage = snapshot.uiPreferences.uiLanguage,
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

    private fun handleBridgeMessage(
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
        val ttsState = overlayTtsState(
            settings = snapshot.ttsSettings,
            runtimeState = lastRuntimeTtsState,
            forceEnabled = snapshot.state.config.transcriptionProvider.id == dev.screengoated.toolbox.mobile.model.RealtimeModelIds.TRANSCRIPTION_GEMINI_S2S,
        )
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
        context.startActivity(ProjectionConsentProxyActivity.startSessionIntent(context))
    }

    private fun updateTargetLanguage(language: String) {
        if (language.isBlank()) {
            return
        }
        repository.updateConfig(LiveSessionPatch(targetLanguage = language))
        languagePicker.hide()
        transcriptionLanguagePicker.hide()
        transcriptionModelPicker.hide()
        translationModelPicker.hide()
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

    private fun currentOverlayLocale() =
        dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText.forLanguage(
            repository.currentUiPreferences().uiLanguage,
        ).overlay

    private val transcriptionModelPicker = dev.screengoated.toolbox.mobile.service.overlay.OverlayLanguagePicker(
        context = context,
        windowManager = windowManager,
        screenBoundsProvider = ::screenBounds,
        onSelected = ::onTranscriptionModelSelected,
    )
    private val translationModelPicker = dev.screengoated.toolbox.mobile.service.overlay.OverlayLanguagePicker(
        context = context,
        windowManager = windowManager,
        screenBoundsProvider = ::screenBounds,
        onSelected = ::onTranslationModelSelected,
    )

    private fun showTranscriptionModelPicker() {
        val anchor = transcriptionWindow?.currentBounds() ?: return
        val geminiS2sLabel = currentOverlayLocale().geminiS2sTitle
        val models = listOf(
            "Gemini Live | 100+ languages",
            geminiS2sLabel,
            "Moonshine Tiny | 1 language",
            "Moonshine Small | 1 language",
            "Moonshine Medium | 1 language",
            "Zipformer | 8 languages",
        )
        val currentId = repository.transcriptionModelId()
        val currentLabel = if (currentId == dev.screengoated.toolbox.mobile.model.RealtimeModelIds.TRANSCRIPTION_GEMINI_S2S) {
            geminiS2sLabel
        } else {
            TRANSCRIPTION_MODEL_LABELS[currentId] ?: "Gemini Live | 100+ languages"
        }
        transcriptionModelPicker.show(
            anchorBounds = anchor,
            selectedLanguage = currentLabel,
            languages = models,
            isDark = isDarkTheme(repository.currentUiPreferences().themeMode),
            title = "Transcription Model",
        )
    }

    private fun onTranscriptionModelSelected(label: String) {
        val modelId = if (label == currentOverlayLocale().geminiS2sTitle) {
            dev.screengoated.toolbox.mobile.model.RealtimeModelIds.TRANSCRIPTION_GEMINI_S2S
        } else {
            TRANSCRIPTION_MODEL_IDS[label] ?: return
        }
        if (repository.transcriptionModelId() != modelId) {
            repository.updateTranscriptionModel(modelId)
            // Reset language to English when switching models to avoid stale
            // language codes that don't exist in the new model's language list
            repository.updateTranscriptionLanguage(
                if (modelId == dev.screengoated.toolbox.mobile.model.RealtimeModelIds.TRANSCRIPTION_GEMINI_S2S ||
                    modelId == dev.screengoated.toolbox.mobile.model.RealtimeModelIds.TRANSCRIPTION_GEMINI_2_5
                ) {
                    "all"
                } else {
                    "en"
                },
            )
            android.os.Handler(android.os.Looper.getMainLooper()).postDelayed({
                restartRequested()
            }, 300)
        }
    }

    private fun showTranslationModelPicker() {
        if (repository.transcriptionModelId() == dev.screengoated.toolbox.mobile.model.RealtimeModelIds.TRANSCRIPTION_GEMINI_S2S) {
            return
        }
        val anchor = translationWindow?.currentBounds() ?: return
        val models = listOf("Gemma", "Cerebras", "GTX")
        val currentId = repository.currentConfig().translationProvider.id
        val currentLabel = TRANSLATION_MODEL_LABELS[currentId] ?: "Gemma"
        translationModelPicker.show(
            anchorBounds = anchor,
            selectedLanguage = currentLabel,
            languages = models,
            isDark = isDarkTheme(repository.currentUiPreferences().themeMode),
            title = "Translation Model",
        )
    }

    private fun onTranslationModelSelected(label: String) {
        val modelId = TRANSLATION_MODEL_IDS[label] ?: return
        repository.updateTranslationModel(modelId)
    }

    private fun showTranscriptionLanguagePicker() {
        val anchor = transcriptionWindow?.currentBounds() ?: return
        val modelId = repository.transcriptionModelId()
        val currentCode = repository.currentConfig().transcriptionLanguage

        // Zipformer has its own language list (8 options)
        if (modelId == "zipformer") {
            val zipLangs = dev.screengoated.toolbox.mobile.service.moonshine.ZipformerLanguage.entries
                .map { it.displayName }
            val currentName = dev.screengoated.toolbox.mobile.service.moonshine.ZipformerLanguage
                .fromCode(currentCode)?.displayName ?: "English"
            transcriptionLanguagePicker.show(
                anchorBounds = anchor,
                selectedLanguage = currentName,
                languages = zipLangs,
                isDark = isDarkTheme(repository.currentUiPreferences().themeMode),
                title = "Zipformer Language",
            )
        }
    }

    private fun onTranscriptionLanguageSelected(selectedName: String) {
        val modelId = repository.transcriptionModelId()
        if (modelId == "zipformer") {
            val lang = dev.screengoated.toolbox.mobile.service.moonshine.ZipformerLanguage.entries
                .find { it.displayName == selectedName }
            if (lang != null) {
                repository.updateTranscriptionLanguage(lang.code)
                // Delay restart to let config propagate before new session reads it
                android.os.Handler(android.os.Looper.getMainLooper()).postDelayed({
                    restartRequested()
                }, 300)
            }
        }
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
        val loaded = OverlayBounds(x = x, y = y, width = width, height = height)
        return if (isNearDismissArea(loaded)) defaults else loaded
    }

    private fun saveBounds(
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

    private fun ensureDismissBubble() {
        if (dismissZone != null) return
        dismissZone = MorphDismissZone(
            context = context,
            windowManager = windowManager,
            targets = dismissTargets,
        ).also { it.show() }
    }

    private fun updateDismissZone(rawXY: String) {
        ensureDismissBubble()
        dismissZone?.update(floatArrayOf(dismissZoneProximity(rawXY)))
    }

    private fun hideDismissZone() {
        dismissZone?.hide()
        dismissZone = null
        resetDismissTracking()
    }

    private fun dismissZoneProximity(rawXY: String): Float {
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

    private fun isNearDismissArea(bounds: OverlayBounds): Boolean {
        val screen = screenBounds()
        val dismissTop = (screen.height() - dp(DISMISS_ZONE_PX)).coerceAtLeast(0)
        return bounds.y + bounds.height >= dismissTop
    }

    private fun resetDismissTracking() {
        lastDismissDistanceSq.fill(Float.POSITIVE_INFINITY)
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
        private const val DRAG_WINDOW_GAIN = 1f
        private const val DISMISS_THRESHOLD = 0.8f
        private const val DISMISS_ZONE_PX = 120
        private const val PERF_TAG = "SGTOverlayPerf"

        // Label ↔ model ID mappings for native pickers
        private val TRANSCRIPTION_MODEL_IDS = mapOf(
            "Gemini Live | 100+ languages" to "gemini-live-audio",
            "Moonshine Tiny | 1 language" to "moonshine-tiny-streaming",
            "Moonshine Small | 1 language" to "moonshine-small-streaming",
            "Moonshine Medium | 1 language" to "moonshine-medium-streaming",
            "Zipformer | 8 languages" to "zipformer",
        )
        private val TRANSCRIPTION_MODEL_LABELS = TRANSCRIPTION_MODEL_IDS.entries.associate { (k, v) -> v to k }

        private val TRANSLATION_MODEL_IDS = mapOf(
            "LLM" to "text-llm",
            "Google Dịch" to "google-gtx",
        )
        private val TRANSLATION_MODEL_LABELS = TRANSLATION_MODEL_IDS.entries.associate { (k, v) -> v to k }
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
