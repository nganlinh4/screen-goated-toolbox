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


// Model/language picker dialogs extracted from OverlayController.
internal fun OverlayController.showLanguagePicker() {
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
        searchHint = locale.overlay.pickerSearchHint,
    )
}

internal fun OverlayController.currentOverlayLocale() =
    dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText.forLanguage(
        repository.currentUiPreferences().uiLanguage,
    ).overlay


internal fun OverlayController.showTranscriptionModelPicker() {
    val anchor = transcriptionWindow?.currentBounds() ?: return
    val overlayLocale = currentOverlayLocale()
    val options = RealtimeOverlayModelOptions.transcriptionOptions(
        geminiS2sLabel = overlayLocale.geminiS2sTitle,
        unavailableSuffix = overlayLocale.unavailableSuffix,
    )
    val currentId = repository.transcriptionModelId()
    val currentLabel = options.firstOrNull { it.id == currentId }?.label ?: options.first().label
    transcriptionModelPicker.showOptions(
        anchorBounds = anchor,
        selectedLanguage = currentLabel,
        options = options.map {
            OverlayPickerOption(
                label = it.label,
                enabled = it.enabled,
            )
        },
        isDark = isDarkTheme(repository.currentUiPreferences().themeMode),
        title = overlayLocale.transcriptionModelTitle,
        searchHint = overlayLocale.pickerSearchHint,
    )
}

internal fun OverlayController.onTranscriptionModelSelected(label: String) {
    val overlayLocale = currentOverlayLocale()
    val modelId = RealtimeOverlayModelOptions.transcriptionOptions(
        geminiS2sLabel = overlayLocale.geminiS2sTitle,
        unavailableSuffix = overlayLocale.unavailableSuffix,
    ).firstOrNull { it.label == label }?.id ?: return
    updateTranscriptionModel(modelId)
}

internal fun OverlayController.showTranslationModelPicker() {
    if (repository.transcriptionModelId() == RealtimeModelIds.TRANSCRIPTION_GEMINI_S2S) {
        return
    }
    val anchor = translationWindow?.currentBounds() ?: return
    val overlayLocale = currentOverlayLocale()
    val options = RealtimeOverlayModelOptions.translationOptions(
        llmLabel = overlayLocale.llmLabel,
        gtxLabel = overlayLocale.gtxLabel,
    )
    val models = options.map { it.label }
    val currentId = repository.currentConfig().translationProvider.id
    val currentLabel = options.firstOrNull { it.id == currentId }?.label ?: options.first().label
    translationModelPicker.show(
        anchorBounds = anchor,
        selectedLanguage = currentLabel,
        languages = models,
        isDark = isDarkTheme(repository.currentUiPreferences().themeMode),
        title = overlayLocale.translationModelTitle,
        searchHint = overlayLocale.pickerSearchHint,
    )
}

internal fun OverlayController.onTranslationModelSelected(label: String) {
    val overlayLocale = currentOverlayLocale()
    val modelId = RealtimeOverlayModelOptions.translationOptions(
        llmLabel = overlayLocale.llmLabel,
        gtxLabel = overlayLocale.gtxLabel,
    ).firstOrNull { it.label == label }?.id ?: return
    repository.updateTranslationModel(modelId)
}

internal fun OverlayController.showTranscriptionLanguagePicker() {
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
            title = currentOverlayLocale().transcriptionLanguageTitle,
            searchHint = currentOverlayLocale().pickerSearchHint,
        )
    }
}

internal fun OverlayController.onTranscriptionLanguageSelected(selectedName: String) {
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

internal fun OverlayController.updateTranscriptionModel(modelId: String) {
    if (repository.transcriptionModelId() != modelId) {
        repository.updateTranscriptionModel(modelId)
        // Reset language to a valid baseline for the newly selected model
        // before the restarted session reads config.
        repository.updateTranscriptionLanguage(defaultTranscriptionLanguageFor(modelId))
        android.os.Handler(android.os.Looper.getMainLooper()).postDelayed({
            restartRequested()
        }, 300)
    }
}

internal fun OverlayController.defaultTranscriptionLanguageFor(modelId: String): String {
    return if (
        modelId == RealtimeModelIds.TRANSCRIPTION_GEMINI_S2S ||
        modelId == RealtimeModelIds.TRANSCRIPTION_GEMINI_2_5
    ) {
        "all"
    } else {
        "en"
    }
}


