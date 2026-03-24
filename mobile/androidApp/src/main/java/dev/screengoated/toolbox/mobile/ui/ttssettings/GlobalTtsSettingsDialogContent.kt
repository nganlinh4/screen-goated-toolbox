@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui.ttssettings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.AutoAwesome
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.GraphicEq
import androidx.compose.material.icons.rounded.Language
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ButtonGroupDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.material3.ToggleButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
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
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

@Composable
internal fun RenderGlobalTtsSettingsDialog(
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
        Card(
            modifier = Modifier
                .fillMaxWidth(0.985f)
                .widthIn(max = 980.dp)
                .padding(16.dp),
            shape = MaterialTheme.shapes.medium,
            colors = androidx.compose.material3.CardDefaults.cardColors(
                containerColor = MaterialTheme.colorScheme.surface,
            ),
        ) {
            BoxWithConstraints(
                modifier = Modifier
                    .fillMaxWidth()
                    .fillMaxHeight(0.96f)
                    .heightIn(max = 900.dp)
                    .padding(start = 24.dp, end = 12.dp, top = 12.dp, bottom = 12.dp),
            ) {
                val isLandscape = maxWidth > maxHeight

                Column(modifier = Modifier.fillMaxWidth()) {
                    // Header: title + toggles (landscape) + close
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Text(
                            text = if (isLandscape) "TTS" else locale.ttsSettingsTitle,
                            style = MaterialTheme.typography.titleLarge,
                            fontWeight = FontWeight.SemiBold,
                        )
                        if (isLandscape) {
                            Spacer(Modifier.weight(1f))
                            MethodToggleRow(settings.method, locale, selectMethod)
                        } else {
                            Spacer(Modifier.weight(1f))
                        }
                        IconButton(onClick = onDismiss) {
                            Icon(Icons.Rounded.Close, contentDescription = locale.closeLabel)
                        }
                    }

                    if (!isLandscape) {
                        Spacer(Modifier.size(8.dp))
                        MethodToggleRow(settings.method, locale, selectMethod)
                    }
                    Spacer(Modifier.size(12.dp))

                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .weight(1f)
                            .verticalScroll(rememberScrollState()),
                        verticalArrangement = Arrangement.spacedBy(16.dp),
                    ) {
                        when (settings.method) {
                            MobileTtsMethod.GEMINI_LIVE -> GeminiLiveSection(
                                settings = settings,
                                locale = locale,
                                onSpeedPresetChanged = onSpeedPresetChanged,
                                onConditionsChanged = onConditionsChanged,
                                onVoiceChanged = onVoiceChanged,
                                onPreviewVoice = onPreviewGeminiVoice,
                            )

                            MobileTtsMethod.GOOGLE_TRANSLATE -> GoogleTranslateSection(
                                selected = settings.speedPreset,
                                locale = locale,
                                onSpeedPresetChanged = onSpeedPresetChanged,
                            )

                            MobileTtsMethod.EDGE_TTS -> EdgeTtsSection(
                                settings = settings.edgeSettings,
                                locale = locale,
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
}

@Composable
private fun MethodToggleRow(
    currentMethod: MobileTtsMethod,
    locale: MobileLocaleText,
    onSelect: (MobileTtsMethod) -> Unit,
) {
    val methods = listOf(
        Triple(MobileTtsMethod.GEMINI_LIVE, locale.ttsMethodStandard, Icons.Rounded.AutoAwesome),
        Triple(MobileTtsMethod.EDGE_TTS, locale.ttsMethodEdge, Icons.Rounded.GraphicEq),
        Triple(MobileTtsMethod.GOOGLE_TRANSLATE, locale.ttsMethodFast, Icons.Rounded.Language),
    )
    FlowRow(
        horizontalArrangement = Arrangement.spacedBy(ButtonGroupDefaults.ConnectedSpaceBetween),
        verticalArrangement = Arrangement.spacedBy(ButtonGroupDefaults.ConnectedSpaceBetween),
    ) {
        methods.forEachIndexed { index, (method, label, icon) ->
            ToggleButton(
                checked = currentMethod == method,
                onCheckedChange = { onSelect(method) },
                shapes = when (index) {
                    0 -> ButtonGroupDefaults.connectedLeadingButtonShapes()
                    methods.lastIndex -> ButtonGroupDefaults.connectedTrailingButtonShapes()
                    else -> ButtonGroupDefaults.connectedMiddleButtonShapes()
                },
                modifier = Modifier.semantics { role = Role.RadioButton },
            ) {
                Icon(icon, contentDescription = null, modifier = Modifier.size(ButtonDefaults.IconSize))
                Spacer(Modifier.size(ButtonDefaults.IconSpacing))
                Text(label, style = MaterialTheme.typography.labelMediumEmphasized, maxLines = 1)
            }
        }
    }
}

@Composable
private fun GoogleTranslateSection(
    selected: MobileTtsSpeedPreset,
    locale: MobileLocaleText,
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
                text = locale.ttsGoogleTranslateTitle,
                style = MaterialTheme.typography.titleLargeEmphasized,
                fontWeight = FontWeight.SemiBold,
            )
            Text(
                text = locale.ttsGoogleTranslateDesc,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            val speedOptions = listOf(
                MobileTtsSpeedPreset.SLOW to locale.ttsSpeedSlow,
                MobileTtsSpeedPreset.NORMAL to locale.ttsSpeedNormal,
            )
            Row(horizontalArrangement = Arrangement.spacedBy(ButtonGroupDefaults.ConnectedSpaceBetween)) {
                speedOptions.forEachIndexed { index, (preset, label) ->
                    ToggleButton(
                        checked = selected == preset,
                        onCheckedChange = { onSpeedPresetChanged(preset) },
                        shapes = when (index) {
                            0 -> ButtonGroupDefaults.connectedLeadingButtonShapes()
                            else -> ButtonGroupDefaults.connectedTrailingButtonShapes()
                        },
                        modifier = Modifier.semantics { role = Role.RadioButton },
                    ) {
                        Text(label)
                    }
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
