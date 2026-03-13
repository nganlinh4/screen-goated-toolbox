@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui.ttssettings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
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
import androidx.compose.material.icons.rounded.AutoAwesome
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.GraphicEq
import androidx.compose.material.icons.rounded.Language
import androidx.compose.material3.Card
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.SingleChoiceSegmentedButtonRowScope
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.WideNavigationRail
import androidx.compose.material3.WideNavigationRailItem
import androidx.compose.material3.WideNavigationRailValue
import androidx.compose.material3.rememberWideNavigationRailState
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
        Surface(
            modifier = Modifier
                .fillMaxWidth(0.985f)
                .widthIn(max = 980.dp),
            shape = MaterialTheme.shapes.extraLarge,
            tonalElevation = 8.dp,
        ) {
            BoxWithConstraints(
                modifier = Modifier
                    .fillMaxWidth()
                    .fillMaxHeight(0.96f)
                    .heightIn(max = 900.dp)
                    .padding(20.dp),
            ) {
                val railState = rememberWideNavigationRailState(
                    if (maxWidth >= 760.dp) WideNavigationRailValue.Expanded else WideNavigationRailValue.Collapsed,
                )
                val railExpanded = maxWidth >= 760.dp
                val compactLayout = maxWidth < 760.dp

                Column(modifier = Modifier.fillMaxWidth()) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(
                            text = locale.ttsSettingsTitle,
                            style = MaterialTheme.typography.titleLargeEmphasized,
                            fontWeight = FontWeight.SemiBold,
                            modifier = Modifier.weight(1f),
                        )
                        IconButton(onClick = onDismiss) {
                            Icon(Icons.Rounded.Close, contentDescription = locale.closeLabel)
                        }
                    }

                    HorizontalDivider(modifier = Modifier.padding(top = 8.dp, bottom = 16.dp))

                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .fillMaxHeight(),
                        horizontalArrangement = Arrangement.spacedBy(18.dp),
                    ) {
                        if (!compactLayout) {
                            Card {
                                WideNavigationRail(
                                    state = railState,
                                    modifier = Modifier.fillMaxHeight(),
                                    header = {
                                        Text(
                                            text = locale.ttsMethodLabel,
                                            modifier = Modifier.padding(horizontal = 18.dp, vertical = 12.dp),
                                            style = MaterialTheme.typography.labelLargeEmphasized,
                                        )
                                    },
                                ) {
                                    WideNavigationRailItem(
                                        selected = settings.method == MobileTtsMethod.GEMINI_LIVE,
                                        onClick = { selectMethod(MobileTtsMethod.GEMINI_LIVE) },
                                        icon = { Icon(Icons.Rounded.AutoAwesome, contentDescription = null) },
                                        label = { Text(locale.ttsMethodStandard) },
                                        railExpanded = railExpanded,
                                    )
                                    WideNavigationRailItem(
                                        selected = settings.method == MobileTtsMethod.EDGE_TTS,
                                        onClick = { selectMethod(MobileTtsMethod.EDGE_TTS) },
                                        icon = { Icon(Icons.Rounded.GraphicEq, contentDescription = null) },
                                        label = { Text(locale.ttsMethodEdge) },
                                        railExpanded = railExpanded,
                                    )
                                    WideNavigationRailItem(
                                        selected = settings.method == MobileTtsMethod.GOOGLE_TRANSLATE,
                                        onClick = { selectMethod(MobileTtsMethod.GOOGLE_TRANSLATE) },
                                        icon = { Icon(Icons.Rounded.Language, contentDescription = null) },
                                        label = { Text(locale.ttsMethodFast) },
                                        railExpanded = railExpanded,
                                    )
                                }
                            }
                        }

                        Column(
                            modifier = Modifier
                                .weight(1f)
                                .verticalScroll(rememberScrollState()),
                            verticalArrangement = Arrangement.spacedBy(16.dp),
                        ) {
                            if (compactLayout) {
                                Card {
                                    Column(
                                        modifier = Modifier
                                            .fillMaxWidth()
                                            .padding(16.dp),
                                        verticalArrangement = Arrangement.spacedBy(12.dp),
                                    ) {
                                        Text(
                                            text = locale.ttsMethodLabel,
                                            style = MaterialTheme.typography.labelLargeEmphasized,
                                        )
                                        SingleChoiceSegmentedButtonRow(
                                            modifier = Modifier.fillMaxWidth(),
                                        ) {
                                            TtsMethodSegment(
                                                index = 0,
                                                count = 3,
                                                selected = settings.method == MobileTtsMethod.GEMINI_LIVE,
                                                label = locale.ttsMethodStandard,
                                            ) { selectMethod(MobileTtsMethod.GEMINI_LIVE) }
                                            TtsMethodSegment(
                                                index = 1,
                                                count = 3,
                                                selected = settings.method == MobileTtsMethod.EDGE_TTS,
                                                label = locale.ttsMethodEdge,
                                            ) { selectMethod(MobileTtsMethod.EDGE_TTS) }
                                            TtsMethodSegment(
                                                index = 2,
                                                count = 3,
                                                selected = settings.method == MobileTtsMethod.GOOGLE_TRANSLATE,
                                                label = locale.ttsMethodFast,
                                            ) { selectMethod(MobileTtsMethod.GOOGLE_TRANSLATE) }
                                        }
                                    }
                                }
                            }
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
            Row(horizontalArrangement = Arrangement.spacedBy(18.dp)) {
                TtsRadioRow(locale.ttsSpeedSlow, selected == MobileTtsSpeedPreset.SLOW) {
                    onSpeedPresetChanged(MobileTtsSpeedPreset.SLOW)
                }
                TtsRadioRow(locale.ttsSpeedNormal, selected == MobileTtsSpeedPreset.NORMAL) {
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

@Composable
private fun SingleChoiceSegmentedButtonRowScope.TtsMethodSegment(
    index: Int,
    count: Int,
    selected: Boolean,
    label: String,
    onClick: () -> Unit,
) {
    SegmentedButton(
        shape = SegmentedButtonDefaults.itemShape(index = index, count = count),
        selected = selected,
        onClick = onClick,
        label = {
            Text(
                text = label,
                maxLines = 2,
                style = MaterialTheme.typography.labelMediumEmphasized,
            )
        },
    )
}

internal fun Int.divCeil(divisor: Int): Int {
    return (this + divisor - 1) / divisor
}
