package dev.screengoated.toolbox.mobile.ui.ttssettings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material3.Card
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import dev.screengoated.toolbox.mobile.model.MobileEdgeTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileTtsLanguageCondition
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset
import dev.screengoated.toolbox.mobile.service.tts.EdgeVoiceCatalogState

@Composable
internal fun RenderGlobalTtsSettingsDialog(
    settings: MobileGlobalTtsSettings,
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
) {
    val selectMethod: (MobileTtsMethod) -> Unit = { method ->
        onMethodChanged(method)
        if (method == MobileTtsMethod.GOOGLE_TRANSLATE && settings.speedPreset == MobileTtsSpeedPreset.FAST) {
            onSpeedPresetChanged(MobileTtsSpeedPreset.NORMAL)
        }
    }

    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false),
    ) {
        Surface(
            modifier = Modifier
                .fillMaxWidth(0.98f)
                .widthIn(max = 940.dp),
            shape = MaterialTheme.shapes.extraLarge,
            tonalElevation = 6.dp,
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(max = 760.dp)
                    .padding(20.dp),
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        text = "Voice Settings",
                        style = MaterialTheme.typography.titleLarge,
                        fontWeight = FontWeight.SemiBold,
                        modifier = Modifier.weight(1f),
                    )
                    IconButton(onClick = onDismiss) {
                        Icon(Icons.Rounded.Close, contentDescription = "Close voice settings")
                    }
                }

                HorizontalDivider(modifier = Modifier.padding(top = 8.dp, bottom = 16.dp))

                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .fillMaxHeight()
                        .verticalScroll(rememberScrollState()),
                    verticalArrangement = Arrangement.spacedBy(16.dp),
                ) {
                    MethodSelector(
                        selected = settings.method,
                        onChanged = selectMethod,
                    )

                    when (settings.method) {
                        MobileTtsMethod.GEMINI_LIVE -> GeminiLiveSection(
                            settings = settings,
                            onSpeedPresetChanged = onSpeedPresetChanged,
                            onConditionsChanged = onConditionsChanged,
                            onVoiceChanged = onVoiceChanged,
                            onPreviewVoice = onPreviewGeminiVoice,
                        )

                        MobileTtsMethod.GOOGLE_TRANSLATE -> GoogleTranslateSection(
                            selected = settings.speedPreset,
                            onSpeedPresetChanged = onSpeedPresetChanged,
                        )

                        MobileTtsMethod.EDGE_TTS -> EdgeTtsSection(
                            settings = settings.edgeSettings,
                            catalogState = edgeVoiceCatalogState,
                            onChanged = onEdgeSettingsChanged,
                            onRetryCatalog = onRetryEdgeVoiceCatalog,
                            onPreviewVoice = onPreviewEdgeVoice,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun MethodSelector(
    selected: MobileTtsMethod,
    onChanged: (MobileTtsMethod) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(
            text = "TTS Method:",
            style = MaterialTheme.typography.labelLarge,
            fontWeight = FontWeight.SemiBold,
        )
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(18.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            TtsRadioRow("Standard (Gemini Live)", selected == MobileTtsMethod.GEMINI_LIVE) {
                onChanged(MobileTtsMethod.GEMINI_LIVE)
            }
            TtsRadioRow("Edge TTS", selected == MobileTtsMethod.EDGE_TTS) {
                onChanged(MobileTtsMethod.EDGE_TTS)
            }
            TtsRadioRow("Fast (Google Translate)", selected == MobileTtsMethod.GOOGLE_TRANSLATE) {
                onChanged(MobileTtsMethod.GOOGLE_TRANSLATE)
            }
        }
    }
}

@Composable
private fun GoogleTranslateSection(
    selected: MobileTtsSpeedPreset,
    onSpeedPresetChanged: (MobileTtsSpeedPreset) -> Unit,
) {
    Card {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(18.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text(
                text = "Google Translate TTS",
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
            )
            Text(
                text = "This method is faster and does not require an API key.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(18.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                TtsRadioRow("Slow", selected == MobileTtsSpeedPreset.SLOW) {
                    onSpeedPresetChanged(MobileTtsSpeedPreset.SLOW)
                }
                TtsRadioRow("Normal", selected == MobileTtsSpeedPreset.NORMAL) {
                    onSpeedPresetChanged(MobileTtsSpeedPreset.NORMAL)
                }
            }
        }
    }
}

@Composable
internal fun TtsRadioRow(
    label: String,
    selected: Boolean,
    onClick: () -> Unit,
) {
    Row(
        modifier = Modifier
            .selectable(selected = selected, onClick = onClick)
            .padding(vertical = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        RadioButton(selected = selected, onClick = onClick)
        Text(text = label, style = MaterialTheme.typography.bodyMedium)
    }
}

internal fun Int.divCeil(divisor: Int): Int {
    return (this + divisor - 1) / divisor
}
