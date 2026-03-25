package dev.screengoated.toolbox.mobile.ui

import androidx.compose.runtime.Composable
import dev.screengoated.toolbox.mobile.model.MobileEdgeTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileTtsLanguageCondition
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset
import dev.screengoated.toolbox.mobile.service.tts.EdgeVoiceCatalogState
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import dev.screengoated.toolbox.mobile.ui.ttssettings.RenderGlobalTtsSettingsDialog

@Composable
fun GlobalTtsSettingsDialog(
    settings: MobileGlobalTtsSettings,
    locale: MobileLocaleText,
    edgeVoiceCatalogState: EdgeVoiceCatalogState,
    onDismiss: () -> Unit,
    onMethodChanged: (MobileTtsMethod) -> Unit,
    onSpeedPresetChanged: (MobileTtsSpeedPreset) -> Unit,
    onVoiceChanged: (String) -> Unit,
    onConditionsChanged: (List<MobileTtsLanguageCondition>) -> Unit,
    onEdgeSettingsChanged: (MobileEdgeTtsSettings) -> Unit,
    onRetryEdgeVoiceCatalog: () -> Unit,
    onPreviewGeminiVoice: (String) -> Unit,
    onPreviewEdgeVoice: (String, String) -> Unit,
    onPreviewGoogleTranslate: () -> Unit,
) {
    RenderGlobalTtsSettingsDialog(
        settings = settings,
        locale = locale,
        edgeVoiceCatalogState = edgeVoiceCatalogState,
        onDismiss = onDismiss,
        onMethodChanged = onMethodChanged,
        onSpeedPresetChanged = onSpeedPresetChanged,
        onVoiceChanged = onVoiceChanged,
        onConditionsChanged = onConditionsChanged,
        onEdgeSettingsChanged = onEdgeSettingsChanged,
        onRetryEdgeVoiceCatalog = onRetryEdgeVoiceCatalog,
        onPreviewGeminiVoice = onPreviewGeminiVoice,
        onPreviewEdgeVoice = onPreviewEdgeVoice,
        onPreviewGoogleTranslate = onPreviewGoogleTranslate,
    )
}
