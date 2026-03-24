@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui.ttssettings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.verticalScroll
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.AutoAwesome
import androidx.compose.material.icons.rounded.GraphicEq
import androidx.compose.material.icons.rounded.Language
import androidx.compose.material3.ButtonGroupDefaults
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.MaterialShapes
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
import dev.screengoated.toolbox.mobile.model.MobileEdgeTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileTtsLanguageCondition
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset
import dev.screengoated.toolbox.mobile.service.tts.EdgeVoiceCatalogState
import dev.screengoated.toolbox.mobile.ui.ExpressiveDialogSectionCard
import dev.screengoated.toolbox.mobile.ui.ExpressiveDialogSurface
import dev.screengoated.toolbox.mobile.ui.ExpressiveMorphPair
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
    val isLandscape =
        LocalConfiguration.current.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE
    val selectMethod: (MobileTtsMethod) -> Unit = { method ->
        onMethodChanged(method)
        if (method == MobileTtsMethod.GOOGLE_TRANSLATE && settings.speedPreset == MobileTtsSpeedPreset.FAST) {
            onSpeedPresetChanged(MobileTtsSpeedPreset.NORMAL)
        }
    }
    val accent = when (settings.method) {
        MobileTtsMethod.GEMINI_LIVE -> MaterialTheme.colorScheme.primary
        MobileTtsMethod.EDGE_TTS -> MaterialTheme.colorScheme.tertiary
        MobileTtsMethod.GOOGLE_TRANSLATE -> MaterialTheme.colorScheme.secondary
    }

    ExpressiveDialogSurface(
        title = locale.ttsSettingsTitle,
        icon = when (settings.method) {
            MobileTtsMethod.GEMINI_LIVE -> Icons.Rounded.AutoAwesome
            MobileTtsMethod.EDGE_TTS -> Icons.Rounded.GraphicEq
            MobileTtsMethod.GOOGLE_TRANSLATE -> Icons.Rounded.Language
        },
        accent = accent,
        morphPair = ExpressiveMorphPair(MaterialShapes.Square, MaterialShapes.Cookie6Sided),
        onDismiss = onDismiss,
        supporting = when (settings.method) {
            MobileTtsMethod.GEMINI_LIVE -> locale.ttsMethodStandard
            MobileTtsMethod.EDGE_TTS -> locale.ttsMethodEdge
            MobileTtsMethod.GOOGLE_TRANSLATE -> locale.ttsMethodFast
        },
        headerTrailing = if (isLandscape) {
            {
                MethodToggleRow(
                    currentMethod = settings.method,
                    locale = locale,
                    onSelect = selectMethod,
                    compact = true,
                )
            }
        } else {
            null
        },
        widthFraction = 0.985f,
        maxWidth = 980.dp,
        heightFraction = 0.96f,
        maxHeight = 900.dp,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            if (!isLandscape) {
                ExpressiveDialogSectionCard(accent = accent) {
                    MethodToggleRow(
                        currentMethod = settings.method,
                        locale = locale,
                        onSelect = selectMethod,
                        compact = true,
                    )
                }
            }

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

@Composable
private fun MethodToggleRow(
    currentMethod: MobileTtsMethod,
    locale: MobileLocaleText,
    onSelect: (MobileTtsMethod) -> Unit,
    compact: Boolean = false,
) {
    val methods = listOf(
        MobileTtsMethod.GEMINI_LIVE to compactMethodLabel(locale, MobileTtsMethod.GEMINI_LIVE, compact),
        MobileTtsMethod.EDGE_TTS to compactMethodLabel(locale, MobileTtsMethod.EDGE_TTS, compact),
        MobileTtsMethod.GOOGLE_TRANSLATE to compactMethodLabel(locale, MobileTtsMethod.GOOGLE_TRANSLATE, compact),
    )
    Row(
        horizontalArrangement = Arrangement.spacedBy(ButtonGroupDefaults.ConnectedSpaceBetween),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        methods.forEachIndexed { index, (method, label) ->
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
                Text(label, style = MaterialTheme.typography.labelMediumEmphasized, maxLines = 1)
            }
        }
    }
}

private fun compactMethodLabel(
    locale: MobileLocaleText,
    method: MobileTtsMethod,
    compact: Boolean,
): String {
    if (!compact) {
        return when (method) {
            MobileTtsMethod.GEMINI_LIVE -> locale.ttsMethodStandard
            MobileTtsMethod.EDGE_TTS -> locale.ttsMethodEdge
            MobileTtsMethod.GOOGLE_TRANSLATE -> locale.ttsMethodFast
        }
    }
    return when {
        locale.ttsMethodFast.contains("Nhanh") -> when (method) {
            MobileTtsMethod.GEMINI_LIVE -> "Xịn"
            MobileTtsMethod.EDGE_TTS -> "Tốt"
            MobileTtsMethod.GOOGLE_TRANSLATE -> "Nhanh"
        }
        locale.ttsMethodFast.contains("빠름") -> when (method) {
            MobileTtsMethod.GEMINI_LIVE -> "표준"
            MobileTtsMethod.EDGE_TTS -> "좋음"
            MobileTtsMethod.GOOGLE_TRANSLATE -> "빠름"
        }
        else -> when (method) {
            MobileTtsMethod.GEMINI_LIVE -> "Standard"
            MobileTtsMethod.EDGE_TTS -> "Edge"
            MobileTtsMethod.GOOGLE_TRANSLATE -> "Fast"
        }
    }
}

@Composable
private fun GoogleTranslateSection(
    selected: MobileTtsSpeedPreset,
    locale: MobileLocaleText,
    onSpeedPresetChanged: (MobileTtsSpeedPreset) -> Unit,
) {
    ExpressiveDialogSectionCard(accent = MaterialTheme.colorScheme.secondary) {
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
