package dev.screengoated.toolbox.mobile.ui.ttssettings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowForward
import androidx.compose.material.icons.rounded.ArrowDropDown
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material3.Card
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.model.MobileEdgeTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileEdgeTtsVoiceConfig
import dev.screengoated.toolbox.mobile.model.MobileTtsCatalog

@Composable
internal fun EdgeTtsSection(
    settings: MobileEdgeTtsSettings,
    onChanged: (MobileEdgeTtsSettings) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Card {
            Column(
                modifier = Modifier.padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text(
                    text = "Microsoft Edge TTS",
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.SemiBold,
                )
                Text(
                    text = "High-quality neural voices. Free, no API key required.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                SliderField(
                    label = "Pitch",
                    value = settings.pitch.toFloat(),
                    valueRange = -50f..50f,
                    valueLabel = "${settings.pitch} Hz",
                    onValueChange = { onChanged(settings.copy(pitch = it.toInt())) },
                )
                SliderField(
                    label = "Rate",
                    value = settings.rate.toFloat(),
                    valueRange = -50f..100f,
                    valueLabel = "${settings.rate}%",
                    onValueChange = { onChanged(settings.copy(rate = it.toInt())) },
                )
                SliderField(
                    label = "Volume",
                    value = settings.volume.toFloat(),
                    valueRange = -50f..50f,
                    valueLabel = "${settings.volume}%",
                    onValueChange = { onChanged(settings.copy(volume = it.toInt())) },
                )
            }
        }

        EdgeVoiceRoutingCard(
            settings = settings,
            onChanged = onChanged,
        )
    }
}

@Composable
private fun EdgeVoiceRoutingCard(
    settings: MobileEdgeTtsSettings,
    onChanged: (MobileEdgeTtsSettings) -> Unit,
) {
    var addMenuExpanded by remember { mutableStateOf(false) }

    Card {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(
                text = "Voice per Language:",
                style = MaterialTheme.typography.labelLarge,
                fontWeight = FontWeight.SemiBold,
            )

            settings.voiceConfigs.forEachIndexed { index, config ->
                EdgeVoiceConfigRow(
                    config = config,
                    onConfigChanged = { nextConfig ->
                        onChanged(
                            settings.copy(
                                voiceConfigs = settings.voiceConfigs.toMutableList().also { list ->
                                    list[index] = nextConfig
                                },
                            ),
                        )
                    },
                    onRemove = {
                        onChanged(settings.copy(voiceConfigs = settings.voiceConfigs.filterIndexed { current, _ -> current != index }))
                    },
                )
            }

            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Box {
                    OutlinedButton(onClick = { addMenuExpanded = true }) {
                        Text("+ Add Voice Config")
                    }
                    DropdownMenu(
                        expanded = addMenuExpanded,
                        onDismissRequest = { addMenuExpanded = false },
                    ) {
                        val usedCodes = settings.voiceConfigs.map { it.languageCode }.toSet()
                        MobileTtsCatalog.edgeConfigLanguages
                            .filterNot { usedCodes.contains(it.code) }
                            .forEach { option ->
                                DropdownMenuItem(
                                    text = { Text(option.name) },
                                    onClick = {
                                        val defaultVoice = MobileTtsCatalog.edgeVoiceSuggestions(option.code).firstOrNull()
                                            ?: "${option.code}-Voice"
                                        onChanged(
                                            settings.copy(
                                                voiceConfigs = settings.voiceConfigs + MobileEdgeTtsVoiceConfig(
                                                    languageCode = option.code,
                                                    languageName = option.name,
                                                    voiceName = defaultVoice,
                                                ),
                                            ),
                                        )
                                        addMenuExpanded = false
                                    },
                                )
                            }
                    }
                }

                OutlinedButton(
                    onClick = { onChanged(MobileEdgeTtsSettings()) },
                ) {
                    Icon(Icons.Rounded.Refresh, contentDescription = null)
                    Text(
                        text = "Reset to Defaults",
                        modifier = Modifier.padding(start = 6.dp),
                    )
                }
            }

            Text(
                text = "Android stores these Edge routing settings now, but playback still uses the current platform TTS runtime.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun EdgeVoiceConfigRow(
    config: MobileEdgeTtsVoiceConfig,
    onConfigChanged: (MobileEdgeTtsVoiceConfig) -> Unit,
    onRemove: () -> Unit,
) {
    var suggestionsExpanded by remember(config.languageCode, config.voiceName) { mutableStateOf(false) }
    val suggestions = MobileTtsCatalog.edgeVoiceSuggestions(config.languageCode)

    BoxWithConstraints {
        val stacked = maxWidth < 620.dp
        if (stacked) {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                ConditionLanguageLabel(config.languageName)
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    OutlinedTextField(
                        modifier = Modifier.weight(1f),
                        value = config.voiceName,
                        onValueChange = { onConfigChanged(config.copy(voiceName = it)) },
                        singleLine = true,
                        label = { Text("Voice") },
                    )
                    if (suggestions.isNotEmpty()) {
                        Box {
                            IconButton(onClick = { suggestionsExpanded = true }) {
                                Icon(Icons.Rounded.ArrowDropDown, contentDescription = "Show suggested voices")
                            }
                            DropdownMenu(
                                expanded = suggestionsExpanded,
                                onDismissRequest = { suggestionsExpanded = false },
                            ) {
                                suggestions.forEach { voice ->
                                    DropdownMenuItem(
                                        text = { Text(voice) },
                                        onClick = {
                                            onConfigChanged(config.copy(voiceName = voice))
                                            suggestionsExpanded = false
                                        },
                                    )
                                }
                            }
                        }
                    }
                    IconButton(onClick = onRemove) {
                        Icon(Icons.Rounded.Close, contentDescription = "Remove voice config")
                    }
                }
            }
        } else {
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                ConditionLanguageLabel(config.languageName, Modifier.width(138.dp))
                Icon(
                    Icons.AutoMirrored.Rounded.ArrowForward,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                OutlinedTextField(
                    modifier = Modifier.weight(1f),
                    value = config.voiceName,
                    onValueChange = { onConfigChanged(config.copy(voiceName = it)) },
                    singleLine = true,
                    label = { Text("Voice") },
                )
                if (suggestions.isNotEmpty()) {
                    Box {
                        IconButton(onClick = { suggestionsExpanded = true }) {
                            Icon(Icons.Rounded.ArrowDropDown, contentDescription = "Show suggested voices")
                        }
                        DropdownMenu(
                            expanded = suggestionsExpanded,
                            onDismissRequest = { suggestionsExpanded = false },
                        ) {
                            suggestions.forEach { voice ->
                                DropdownMenuItem(
                                    text = { Text(voice) },
                                    onClick = {
                                        onConfigChanged(config.copy(voiceName = voice))
                                        suggestionsExpanded = false
                                    },
                                )
                            }
                        }
                    }
                }
                IconButton(onClick = onRemove) {
                    Icon(Icons.Rounded.Close, contentDescription = "Remove voice config")
                }
            }
        }
    }
}

@Composable
private fun SliderField(
    label: String,
    value: Float,
    valueRange: ClosedFloatingPointRange<Float>,
    valueLabel: String,
    onValueChange: (Float) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = label,
                style = MaterialTheme.typography.bodyMedium,
                modifier = Modifier.weight(1f),
            )
            Text(
                text = valueLabel,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Slider(value = value, onValueChange = onValueChange, valueRange = valueRange)
    }
}
