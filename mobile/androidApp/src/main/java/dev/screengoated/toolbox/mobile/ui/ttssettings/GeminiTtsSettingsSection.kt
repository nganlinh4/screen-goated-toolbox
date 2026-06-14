@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui.ttssettings

import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.ui.res.painterResource
import dev.screengoated.toolbox.mobile.R
import androidx.compose.material3.ButtonGroupDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.material3.ToggleButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileTtsCatalog
import dev.screengoated.toolbox.mobile.model.MobileTtsLanguageCondition
import dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset
import dev.screengoated.toolbox.mobile.translationgummy.TranslationGummyVolumeState
import dev.screengoated.toolbox.mobile.ui.ExpressiveDialogSectionCard
import dev.screengoated.toolbox.mobile.ui.UtilityActionButton
import dev.screengoated.toolbox.mobile.ui.UtilityHeaderRow
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import kotlin.math.roundToInt

@Composable
internal fun GeminiLiveModelAndVoiceOnly(
    settings: MobileGlobalTtsSettings,
    locale: MobileLocaleText,
    onModelChanged: (String) -> Unit,
    onVoiceChanged: (String) -> Unit,
    onPreviewVoice: (String) -> Unit,
    translationGummyVolume: TranslationGummyVolumeState? = null,
    onTranslationGummyVolumeChanged: (Int) -> Unit = {},
    onTranslationGummyMuteToggle: () -> Unit = {},
) {
    Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
        GeminiModelCard(
            selected = settings.geminiModel,
            locale = locale,
            onChanged = onModelChanged,
        )
        translationGummyVolume?.let { volume ->
            TranslationGummyVolumeCard(
                volume = volume,
                locale = locale,
                onVolumeChanged = onTranslationGummyVolumeChanged,
                onMuteToggle = onTranslationGummyMuteToggle,
            )
        }
        ExpressiveDialogSectionCard(accent = MaterialTheme.colorScheme.tertiary) {
            UtilityHeaderRow(
                icon = R.drawable.ms_volume_up,
                title = locale.ttsVoiceLabel,
                accent = MaterialTheme.colorScheme.tertiary,
            )
            GeminiVoiceGrid(
                selectedVoice = settings.voice,
                locale = locale,
                onVoiceChanged = onVoiceChanged,
                onPreviewVoice = onPreviewVoice,
            )
        }
    }
}

@Composable
private fun TranslationGummyVolumeCard(
    volume: TranslationGummyVolumeState,
    locale: MobileLocaleText,
    onVolumeChanged: (Int) -> Unit,
    onMuteToggle: () -> Unit,
) {
    val accent = MaterialTheme.colorScheme.primary
    ExpressiveDialogSectionCard(accent = accent) {
        UtilityHeaderRow(
            icon = if (volume.muted) R.drawable.ms_volume_off else R.drawable.ms_volume_up,
            title = locale.overlay.ttsVolume,
            accent = accent,
            trailing = {
                Text(
                    text = "${volume.percent}%",
                    style = MaterialTheme.typography.labelLarge,
                    fontWeight = FontWeight.SemiBold,
                    color = accent,
                    modifier = Modifier.testTag("translation-gummy-volume-value"),
                )
                IconButton(
                    onClick = onMuteToggle,
                    modifier = Modifier.testTag("translation-gummy-volume-mute"),
                ) {
                    Icon(
                        painterResource(if (volume.muted) R.drawable.ms_volume_off else R.drawable.ms_volume_up),
                        contentDescription = locale.overlay.ttsVolume,
                        tint = accent,
                        modifier = Modifier.size(20.dp),
                    )
                }
            },
        )
        Slider(
            value = volume.percent.toFloat(),
            onValueChange = { rawValue ->
                val snapped = ((rawValue / TranslationGummyVolumeState.STEP_PERCENT).roundToInt() *
                    TranslationGummyVolumeState.STEP_PERCENT)
                    .coerceIn(
                        TranslationGummyVolumeState.MIN_VOLUME_PERCENT,
                        TranslationGummyVolumeState.MAX_VOLUME_PERCENT,
                    )
                onVolumeChanged(snapped)
            },
            valueRange = TranslationGummyVolumeState.MIN_VOLUME_PERCENT.toFloat()..
                TranslationGummyVolumeState.MAX_VOLUME_PERCENT.toFloat(),
            steps = (TranslationGummyVolumeState.MAX_VOLUME_PERCENT /
                TranslationGummyVolumeState.STEP_PERCENT) - 1,
            modifier = Modifier.testTag("translation-gummy-volume-slider"),
        )
    }
}

@Composable
internal fun GeminiLiveSection(
    settings: MobileGlobalTtsSettings,
    locale: MobileLocaleText,
    onModelChanged: (String) -> Unit,
    onSpeedPresetChanged: (MobileTtsSpeedPreset) -> Unit,
    onConditionsChanged: (List<MobileTtsLanguageCondition>) -> Unit,
    onVoiceChanged: (String) -> Unit,
    onPreviewVoice: (String) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
        GeminiModelCard(
            selected = settings.geminiModel,
            locale = locale,
            onChanged = onModelChanged,
        )

        BoxWithConstraints {
            val stacked = maxWidth < 720.dp
            if (stacked) {
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    GeminiSpeedCard(
                        selected = settings.speedPreset,
                        locale = locale,
                        onChanged = onSpeedPresetChanged,
                    )
                    GeminiConditionsCard(settings.languageConditions, locale, onConditionsChanged)
                }
            } else {
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    GeminiSpeedCard(
                        selected = settings.speedPreset,
                        locale = locale,
                        onChanged = onSpeedPresetChanged,
                        modifier = Modifier.weight(0.38f),
                    )
                    GeminiConditionsCard(
                        conditions = settings.languageConditions,
                        locale = locale,
                        onChanged = onConditionsChanged,
                        modifier = Modifier.weight(0.62f),
                    )
                }
            }
        }

        ExpressiveDialogSectionCard(accent = MaterialTheme.colorScheme.tertiary) {
            UtilityHeaderRow(
                icon = R.drawable.ms_volume_up,
                title = locale.ttsVoiceLabel,
                accent = MaterialTheme.colorScheme.tertiary,
            )
            GeminiVoiceGrid(
                selectedVoice = settings.voice,
                locale = locale,
                onVoiceChanged = onVoiceChanged,
                onPreviewVoice = onPreviewVoice,
            )
        }
    }
}

@Composable
private fun GeminiModelCard(
    selected: String,
    locale: MobileLocaleText,
    onChanged: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    ExpressiveDialogSectionCard(
        accent = MaterialTheme.colorScheme.tertiary,
        modifier = modifier,
    ) {
        UtilityHeaderRow(
            icon = R.drawable.ms_auto_awesome,
            title = locale.ttsGeminiModelLabel,
            accent = MaterialTheme.colorScheme.tertiary,
        )
        val modelOptions = MobileTtsCatalog.geminiModels
        SegmentedToggleRow(
            options = modelOptions,
            selectedIndex = modelOptions.indexOfFirst { selected == it.apiModel },
            onSelect = { index -> onChanged(modelOptions[index].apiModel) },
            labelOf = { it.label },
        )
    }
}

@Composable
private fun <T> SegmentedToggleRow(
    options: List<T>,
    selectedIndex: Int,
    onSelect: (Int) -> Unit,
    labelOf: (T) -> String,
) {
    Row(horizontalArrangement = Arrangement.spacedBy(ButtonGroupDefaults.ConnectedSpaceBetween)) {
        options.forEachIndexed { index, option ->
            ToggleButton(
                checked = selectedIndex == index,
                onCheckedChange = { onSelect(index) },
                shapes = when (index) {
                    0 -> ButtonGroupDefaults.connectedLeadingButtonShapes()
                    options.lastIndex -> ButtonGroupDefaults.connectedTrailingButtonShapes()
                    else -> ButtonGroupDefaults.connectedMiddleButtonShapes()
                },
                modifier = Modifier.semantics { role = Role.RadioButton },
            ) {
                Text(labelOf(option), style = MaterialTheme.typography.labelSmall)
            }
        }
    }
}

@Composable
private fun GeminiSpeedCard(
    selected: MobileTtsSpeedPreset,
    locale: MobileLocaleText,
    onChanged: (MobileTtsSpeedPreset) -> Unit,
    modifier: Modifier = Modifier,
) {
    ExpressiveDialogSectionCard(
        accent = MaterialTheme.colorScheme.primary,
        modifier = modifier,
    ) {
        UtilityHeaderRow(
            icon = R.drawable.ms_auto_awesome,
            title = locale.ttsSpeedLabel,
            accent = MaterialTheme.colorScheme.primary,
        )
        val speedOptions = listOf(
            MobileTtsSpeedPreset.SLOW to locale.ttsSpeedSlow,
            MobileTtsSpeedPreset.NORMAL to locale.ttsSpeedNormal,
            MobileTtsSpeedPreset.FAST to locale.ttsSpeedFast,
        )
        SegmentedToggleRow(
            options = speedOptions,
            selectedIndex = speedOptions.indexOfFirst { selected == it.first },
            onSelect = { index -> onChanged(speedOptions[index].first) },
            labelOf = { it.second },
        )
    }
}

@Composable
private fun GeminiConditionsCard(
    conditions: List<MobileTtsLanguageCondition>,
    locale: MobileLocaleText,
    onChanged: (List<MobileTtsLanguageCondition>) -> Unit,
    modifier: Modifier = Modifier,
) {
    var addMenuExpanded by remember { mutableStateOf(false) }

    ExpressiveDialogSectionCard(
        accent = MaterialTheme.colorScheme.secondary,
        modifier = modifier,
    ) {
        UtilityHeaderRow(
            icon = R.drawable.ms_translate,
            title = locale.ttsInstructionsLabel,
            accent = MaterialTheme.colorScheme.secondary,
        )

        if (conditions.isEmpty()) {
            Text(
                text = locale.noLanguageConditionsYet,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }

        conditions.forEachIndexed { index, condition ->
            BoxWithConstraints {
                val stacked = maxWidth < 560.dp
                if (stacked) {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        ConditionLanguageLabel(condition.languageName)
                        OutlinedTextField(
                            modifier = Modifier.fillMaxWidth(),
                            value = condition.instruction,
                            onValueChange = { instruction ->
                                onChanged(
                                    conditions.toMutableList().also { list ->
                                        list[index] = condition.copy(instruction = instruction)
                                    },
                                )
                            },
                            singleLine = true,
                            label = { Text(locale.instructionLabel) },
                        )
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.End,
                        ) {
                            IconButton(
                                onClick = {
                                    onChanged(conditions.filterIndexed { current, _ -> current != index })
                                },
                            ) {
                                Icon(painterResource(R.drawable.ms_close), contentDescription = locale.removeLabel)
                            }
                        }
                    }
                } else {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        ConditionLanguageLabel(condition.languageName, Modifier.width(128.dp))
                        Icon(
                            painterResource(R.drawable.ms_arrow_forward),
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        OutlinedTextField(
                            modifier = Modifier.weight(1f),
                            value = condition.instruction,
                            onValueChange = { instruction ->
                                onChanged(
                                    conditions.toMutableList().also { list ->
                                        list[index] = condition.copy(instruction = instruction)
                                    },
                                )
                            },
                            singleLine = true,
                            label = { Text(locale.instructionLabel) },
                        )
                        IconButton(
                            onClick = {
                                onChanged(conditions.filterIndexed { current, _ -> current != index })
                            },
                        ) {
                            Icon(painterResource(R.drawable.ms_close), contentDescription = locale.removeLabel)
                        }
                    }
                }
            }
        }

        Box {
            UtilityActionButton(
                text = locale.ttsAddCondition,
                accent = MaterialTheme.colorScheme.secondary,
                onClick = { addMenuExpanded = true },
            )
            DropdownMenu(
                expanded = addMenuExpanded,
                onDismissRequest = { addMenuExpanded = false },
            ) {
                val usedCodes = conditions.map { it.languageCode }.toSet()
                MobileTtsCatalog.conditionLanguages
                    .filterNot { usedCodes.contains(it.code) }
                    .forEach { option ->
                        DropdownMenuItem(
                            text = { Text(option.name) },
                            onClick = {
                                onChanged(
                                    conditions + MobileTtsLanguageCondition(
                                        languageCode = option.code,
                                        languageName = option.name,
                                        instruction = "",
                                    ),
                                )
                                addMenuExpanded = false
                            },
                        )
                    }
            }
        }
    }
}

@Composable
internal fun ConditionLanguageLabel(
    name: String,
    modifier: Modifier = Modifier,
) {
    Text(
        text = name,
        modifier = modifier,
        style = MaterialTheme.typography.bodyMedium,
        fontWeight = FontWeight.SemiBold,
        color = MaterialTheme.colorScheme.tertiary,
    )
}
