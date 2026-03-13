package dev.screengoated.toolbox.mobile.ui.ttssettings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowForward
import androidx.compose.material.icons.automirrored.rounded.VolumeUp
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material3.Card
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.model.GeminiVoiceOption
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileTtsCatalog
import dev.screengoated.toolbox.mobile.model.MobileTtsLanguageCondition
import dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

@Composable
internal fun GeminiLiveSection(
    settings: MobileGlobalTtsSettings,
    locale: MobileLocaleText,
    onSpeedPresetChanged: (MobileTtsSpeedPreset) -> Unit,
    onConditionsChanged: (List<MobileTtsLanguageCondition>) -> Unit,
    onVoiceChanged: (String) -> Unit,
    onPreviewVoice: (String) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
        BoxWithConstraints {
            val stacked = maxWidth < 720.dp
            if (stacked) {
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    GeminiSpeedCard(settings.speedPreset, locale, onSpeedPresetChanged)
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

        HorizontalDivider()
        GeminiVoiceGrid(
            selectedVoice = settings.voice,
            locale = locale,
            onVoiceChanged = onVoiceChanged,
            onPreviewVoice = onPreviewVoice,
        )
    }
}

@Composable
private fun GeminiSpeedCard(
    selected: MobileTtsSpeedPreset,
    locale: MobileLocaleText,
    onChanged: (MobileTtsSpeedPreset) -> Unit,
    modifier: Modifier = Modifier,
) {
    Card(modifier = modifier) {
        Column(
            modifier = Modifier.padding(14.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(
                text = locale.ttsSpeedLabel,
                style = MaterialTheme.typography.labelLarge,
                fontWeight = FontWeight.SemiBold,
            )
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(16.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                TtsRadioRow(locale.ttsSpeedSlow, selected == MobileTtsSpeedPreset.SLOW) {
                    onChanged(MobileTtsSpeedPreset.SLOW)
                }
                TtsRadioRow(locale.ttsSpeedNormal, selected == MobileTtsSpeedPreset.NORMAL) {
                    onChanged(MobileTtsSpeedPreset.NORMAL)
                }
                TtsRadioRow(locale.ttsSpeedFast, selected == MobileTtsSpeedPreset.FAST) {
                    onChanged(MobileTtsSpeedPreset.FAST)
                }
            }
        }
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

    Card(modifier = modifier) {
        Column(
            modifier = Modifier.padding(14.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Text(
                text = locale.ttsInstructionsLabel,
                style = MaterialTheme.typography.labelLarge,
                fontWeight = FontWeight.SemiBold,
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
                                    Icon(Icons.Rounded.Close, contentDescription = locale.removeLabel)
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
                                Icons.AutoMirrored.Rounded.ArrowForward,
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
                                Icon(Icons.Rounded.Close, contentDescription = locale.removeLabel)
                            }
                        }
                    }
                }
            }

            Box {
                OutlinedButton(onClick = { addMenuExpanded = true }) {
                    Text(locale.ttsAddCondition)
                }
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

@Composable
private fun GeminiVoiceGrid(
    selectedVoice: String,
    locale: MobileLocaleText,
    onVoiceChanged: (String) -> Unit,
    onPreviewVoice: (String) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Text(
            text = locale.ttsVoiceLabel,
            style = MaterialTheme.typography.labelLarge,
            fontWeight = FontWeight.SemiBold,
        )

        BoxWithConstraints {
            when {
                maxWidth >= 900.dp -> FourColumnVoiceGrid(selectedVoice, locale, onVoiceChanged, onPreviewVoice)
                maxWidth >= 600.dp -> TwoColumnVoiceGrid(selectedVoice, locale, onVoiceChanged, onPreviewVoice)
                else -> SingleColumnVoiceGrid(selectedVoice, locale, onVoiceChanged, onPreviewVoice)
            }
        }
    }
}

@Composable
private fun FourColumnVoiceGrid(
    selectedVoice: String,
    locale: MobileLocaleText,
    onVoiceChanged: (String) -> Unit,
    onPreviewVoice: (String) -> Unit,
) {
    val maleVoices = MobileTtsCatalog.maleVoices
    val femaleVoices = MobileTtsCatalog.femaleVoices
    val maleMid = maleVoices.size.divCeil(2)
    val femaleMid = femaleVoices.size.divCeil(2)

    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        VoiceColumnCard(
            title = locale.ttsMale,
            voices = maleVoices.take(maleMid),
            selectedVoice = selectedVoice,
            locale = locale,
            onVoiceChanged = onVoiceChanged,
            onPreviewVoice = onPreviewVoice,
            modifier = Modifier.weight(1f),
        )
        VoiceColumnCard(
            title = null,
            voices = maleVoices.drop(maleMid),
            selectedVoice = selectedVoice,
            locale = locale,
            onVoiceChanged = onVoiceChanged,
            onPreviewVoice = onPreviewVoice,
            modifier = Modifier.weight(1f),
        )
        VoiceColumnCard(
            title = locale.ttsFemale,
            voices = femaleVoices.take(femaleMid),
            selectedVoice = selectedVoice,
            locale = locale,
            onVoiceChanged = onVoiceChanged,
            onPreviewVoice = onPreviewVoice,
            modifier = Modifier.weight(1f),
        )
        VoiceColumnCard(
            title = null,
            voices = femaleVoices.drop(femaleMid),
            selectedVoice = selectedVoice,
            locale = locale,
            onVoiceChanged = onVoiceChanged,
            onPreviewVoice = onPreviewVoice,
            modifier = Modifier.weight(1f),
        )
    }
}

@Composable
private fun TwoColumnVoiceGrid(
    selectedVoice: String,
    locale: MobileLocaleText,
    onVoiceChanged: (String) -> Unit,
    onPreviewVoice: (String) -> Unit,
) {
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        VoiceColumnCard(
            title = locale.ttsMale,
            voices = MobileTtsCatalog.maleVoices,
            selectedVoice = selectedVoice,
            locale = locale,
            onVoiceChanged = onVoiceChanged,
            onPreviewVoice = onPreviewVoice,
            modifier = Modifier.weight(1f),
        )
        VoiceColumnCard(
            title = locale.ttsFemale,
            voices = MobileTtsCatalog.femaleVoices,
            selectedVoice = selectedVoice,
            locale = locale,
            onVoiceChanged = onVoiceChanged,
            onPreviewVoice = onPreviewVoice,
            modifier = Modifier.weight(1f),
        )
    }
}

@Composable
private fun SingleColumnVoiceGrid(
    selectedVoice: String,
    locale: MobileLocaleText,
    onVoiceChanged: (String) -> Unit,
    onPreviewVoice: (String) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        VoiceColumnCard(
            title = locale.ttsMale,
            voices = MobileTtsCatalog.maleVoices,
            selectedVoice = selectedVoice,
            locale = locale,
            onVoiceChanged = onVoiceChanged,
            onPreviewVoice = onPreviewVoice,
        )
        VoiceColumnCard(
            title = locale.ttsFemale,
            voices = MobileTtsCatalog.femaleVoices,
            selectedVoice = selectedVoice,
            locale = locale,
            onVoiceChanged = onVoiceChanged,
            onPreviewVoice = onPreviewVoice,
        )
    }
}

@Composable
private fun VoiceColumnCard(
    title: String?,
    voices: List<GeminiVoiceOption>,
    selectedVoice: String,
    locale: MobileLocaleText,
    onVoiceChanged: (String) -> Unit,
    onPreviewVoice: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    Card(modifier = modifier) {
        Column(
            modifier = Modifier.padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            if (title != null) {
                Text(
                    text = title,
                    style = MaterialTheme.typography.labelLarge,
                    fontWeight = FontWeight.SemiBold,
                    textDecoration = TextDecoration.Underline,
                )
            }
            voices.forEach { voice ->
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    RadioButton(
                        selected = selectedVoice == voice.name,
                        onClick = { onVoiceChanged(voice.name) },
                    )
                    IconButton(
                        onClick = {
                            onVoiceChanged(voice.name)
                            onPreviewVoice(voice.name)
                        },
                    ) {
                        Icon(
                            Icons.AutoMirrored.Rounded.VolumeUp,
                            contentDescription = locale.ttsVoiceLabel,
                        )
                    }
                    Text(
                        text = voice.name,
                        style = MaterialTheme.typography.bodyMedium,
                        fontWeight = FontWeight.Medium,
                    )
                }
            }
        }
    }
}
