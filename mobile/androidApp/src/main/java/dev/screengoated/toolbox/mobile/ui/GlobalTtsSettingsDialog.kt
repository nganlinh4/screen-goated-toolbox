package dev.screengoated.toolbox.mobile.ui

import androidx.compose.runtime.Composable
import dev.screengoated.toolbox.mobile.model.MobileEdgeTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileTtsLanguageCondition
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset
import dev.screengoated.toolbox.mobile.ui.ttssettings.RenderGlobalTtsSettingsDialog

@Composable
fun GlobalTtsSettingsDialog(
    settings: MobileGlobalTtsSettings,
    onDismiss: () -> Unit,
    onMethodChanged: (MobileTtsMethod) -> Unit,
    onSpeedPresetChanged: (MobileTtsSpeedPreset) -> Unit,
    onVoiceChanged: (String) -> Unit,
    onConditionsChanged: (List<MobileTtsLanguageCondition>) -> Unit,
    onEdgeSettingsChanged: (MobileEdgeTtsSettings) -> Unit,
) {
    RenderGlobalTtsSettingsDialog(
        settings = settings,
        onDismiss = onDismiss,
        onMethodChanged = onMethodChanged,
        onSpeedPresetChanged = onSpeedPresetChanged,
        onVoiceChanged = onVoiceChanged,
        onConditionsChanged = onConditionsChanged,
        onEdgeSettingsChanged = onEdgeSettingsChanged,
    )
}
