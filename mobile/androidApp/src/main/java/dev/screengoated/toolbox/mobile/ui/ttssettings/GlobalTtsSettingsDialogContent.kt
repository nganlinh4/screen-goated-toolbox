@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui.ttssettings

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.wrapContentWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.verticalScroll
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.VolumeUp
import androidx.compose.material.icons.rounded.AutoAwesome
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.GraphicEq
import androidx.compose.material.icons.rounded.Language
import androidx.compose.material3.ButtonGroupDefaults
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialShapes
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.ToggleButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.material3.toShape
import dev.screengoated.toolbox.mobile.model.MobileEdgeTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileTtsLanguageCondition
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset
import dev.screengoated.toolbox.mobile.service.tts.EdgeVoiceCatalogState
import dev.screengoated.toolbox.mobile.ui.ExpressiveDialogSectionCard
import dev.screengoated.toolbox.mobile.ui.ExpressiveDialogSurface
import dev.screengoated.toolbox.mobile.ui.ExpressiveMorphPair
import dev.screengoated.toolbox.mobile.ui.UtilityActionButton
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

@Composable
internal fun RenderGlobalTtsSettingsDialog(
    settings: MobileGlobalTtsSettings,
    locale: MobileLocaleText,
    edgeVoiceCatalogState: EdgeVoiceCatalogState,
    onDismiss: () -> Unit,
    onMethodChanged: (MobileTtsMethod) -> Unit,
    onGeminiModelChanged: (String) -> Unit,
    onSpeedPresetChanged: (MobileTtsSpeedPreset) -> Unit,
    onVoiceChanged: (String) -> Unit,
    onConditionsChanged: (List<MobileTtsLanguageCondition>) -> Unit,
    onEdgeSettingsChanged: (MobileEdgeTtsSettings) -> Unit,
    onRetryEdgeVoiceCatalog: () -> Unit,
    onPreviewGeminiVoice: (String) -> Unit,
    onPreviewEdgeVoice: (String, String) -> Unit,
    onPreviewGoogleTranslate: () -> Unit,
    geminiOnly: Boolean = false,
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
        headerTrailing = if (isLandscape && !geminiOnly) {
            {
                MethodToggleRow(
                    currentMethod = settings.method,
                    locale = locale,
                    onSelect = selectMethod,
                    compact = true,
                    centered = false,
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
            if (!isLandscape && !geminiOnly) {
                MethodToggleRow(
                    currentMethod = settings.method,
                    locale = locale,
                    onSelect = selectMethod,
                    compact = true,
                    centered = true,
                )
            }

            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f)
                    .verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                if (geminiOnly) {
                    // Distilled: model + voice only (for Translation Gummy gear)
                    GeminiLiveModelAndVoiceOnly(
                        settings = settings,
                        locale = locale,
                        onModelChanged = onGeminiModelChanged,
                        onVoiceChanged = onVoiceChanged,
                        onPreviewVoice = onPreviewGeminiVoice,
                    )
                } else {
                    when (settings.method) {
                        MobileTtsMethod.GEMINI_LIVE -> GeminiLiveSection(
                            settings = settings,
                            locale = locale,
                            onModelChanged = onGeminiModelChanged,
                            onSpeedPresetChanged = onSpeedPresetChanged,
                            onConditionsChanged = onConditionsChanged,
                            onVoiceChanged = onVoiceChanged,
                            onPreviewVoice = onPreviewGeminiVoice,
                        )

                        MobileTtsMethod.GOOGLE_TRANSLATE -> GoogleTranslateSection(
                            selected = settings.speedPreset,
                            locale = locale,
                            onSpeedPresetChanged = onSpeedPresetChanged,
                            onPreview = onPreviewGoogleTranslate,
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

@Composable
private fun MethodToggleRow(
    currentMethod: MobileTtsMethod,
    locale: MobileLocaleText,
    onSelect: (MobileTtsMethod) -> Unit,
    compact: Boolean = false,
    centered: Boolean = false,
) {
    val methods = listOf(
        MobileTtsMethod.GEMINI_LIVE to compactMethodLabel(locale, MobileTtsMethod.GEMINI_LIVE, compact),
        MobileTtsMethod.EDGE_TTS to compactMethodLabel(locale, MobileTtsMethod.EDGE_TTS, compact),
        MobileTtsMethod.GOOGLE_TRANSLATE to compactMethodLabel(locale, MobileTtsMethod.GOOGLE_TRANSLATE, compact),
    )
    val activeBg = MaterialTheme.colorScheme.primaryContainer
    val inactiveBg = Color.Transparent
    val activeContent = MaterialTheme.colorScheme.onPrimaryContainer
    val inactiveContent = MaterialTheme.colorScheme.onSurfaceVariant
    Row(
        modifier = if (centered) Modifier.fillMaxWidth() else Modifier.wrapContentWidth(),
        horizontalArrangement = if (centered) Arrangement.Center else Arrangement.spacedBy(4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(
            modifier = Modifier.wrapContentWidth(),
            horizontalArrangement = Arrangement.spacedBy(if (compact) 4.dp else 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            methods.forEach { (method, label) ->
                val selected = currentMethod == method
                val bgColor by animateColorAsState(
                    targetValue = if (selected) activeBg else inactiveBg,
                    animationSpec = spring(
                        dampingRatio = Spring.DampingRatioMediumBouncy,
                        stiffness = Spring.StiffnessMediumLow,
                    ),
                    label = "tts-method-bg-$label",
                )
                val contentColor by animateColorAsState(
                    targetValue = if (selected) activeContent else inactiveContent,
                    animationSpec = spring(
                        dampingRatio = Spring.DampingRatioMediumBouncy,
                        stiffness = Spring.StiffnessMediumLow,
                    ),
                    label = "tts-method-content-$label",
                )
                val iconBg by animateColorAsState(
                    targetValue = if (selected) MaterialTheme.colorScheme.secondaryContainer else MaterialTheme.colorScheme.surfaceContainerHighest.copy(alpha = 0.62f),
                    animationSpec = spring(
                        dampingRatio = Spring.DampingRatioMediumBouncy,
                        stiffness = Spring.StiffnessMediumLow,
                    ),
                    label = "tts-method-icon-$label",
                )
                val scale by animateFloatAsState(
                    targetValue = if (selected) 1f else 0.985f,
                    animationSpec = spring(
                        dampingRatio = Spring.DampingRatioMediumBouncy,
                        stiffness = Spring.StiffnessMediumLow,
                    ),
                    label = "tts-method-scale-$label",
                )

                Surface(
                    onClick = { onSelect(method) },
                    color = bgColor,
                    contentColor = contentColor,
                    tonalElevation = if (selected) 3.dp else 0.dp,
                    shadowElevation = if (selected) 5.dp else 0.dp,
                    shape = MaterialTheme.shapes.extraLarge,
                    modifier = Modifier.graphicsLayer {
                        scaleX = scale
                        scaleY = scale
                    },
                ) {
                    Row(
                        modifier = Modifier.padding(
                            horizontal = if (compact) 8.dp else 10.dp,
                            vertical = if (compact) 7.dp else 8.dp,
                        ),
                        horizontalArrangement = Arrangement.Center,
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        AnimatedVisibility(
                            visible = selected,
                            enter = androidx.compose.animation.fadeIn() +
                                androidx.compose.animation.expandHorizontally(),
                            exit = androidx.compose.animation.fadeOut() +
                                androidx.compose.animation.shrinkHorizontally(),
                        ) {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Surface(
                                    color = iconBg,
                                    shape = MaterialShapes.Sunny.toShape(),
                                    modifier = Modifier.size(if (compact) 24.dp else 28.dp),
                                ) {
                                    Row(
                                        modifier = Modifier.fillMaxWidth(),
                                        horizontalArrangement = Arrangement.Center,
                                        verticalAlignment = Alignment.CenterVertically,
                                    ) {
                                        Icon(
                                            Icons.Rounded.Check,
                                            contentDescription = null,
                                            modifier = Modifier.size(if (compact) 14.dp else 16.dp),
                                        )
                                    }
                                }
                                Spacer(Modifier.size(if (compact) 6.dp else 8.dp))
                            }
                        }
                        Text(
                            text = label,
                            maxLines = 1,
                            style = MaterialTheme.typography.labelLarge,
                            fontWeight = if (selected) FontWeight.Bold else FontWeight.Medium,
                        )
                    }
                }
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
            MobileTtsMethod.GOOGLE_TRANSLATE -> "Google Trans."
        }
    }
}

@Composable
private fun GoogleTranslateSection(
    selected: MobileTtsSpeedPreset,
    locale: MobileLocaleText,
    onSpeedPresetChanged: (MobileTtsSpeedPreset) -> Unit,
    onPreview: () -> Unit,
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
            UtilityActionButton(
                text = locale.ttsPreviewAction,
                accent = MaterialTheme.colorScheme.secondary,
                onClick = onPreview,
            ) {
                Icon(
                    Icons.AutoMirrored.Rounded.VolumeUp,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.secondary,
                )
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
