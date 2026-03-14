@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

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
import androidx.compose.material.icons.automirrored.rounded.VolumeUp
import androidx.compose.material3.Card
import androidx.compose.material3.ContainedLoadingIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
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
import dev.screengoated.toolbox.mobile.service.tts.EdgeVoiceCatalogState
import dev.screengoated.toolbox.mobile.model.MobileEdgeTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileEdgeTtsVoiceConfig
import dev.screengoated.toolbox.mobile.model.MobileTtsCatalog
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

@Composable
internal fun EdgeTtsSection(
    settings: MobileEdgeTtsSettings,
    locale: MobileLocaleText,
    catalogState: EdgeVoiceCatalogState,
    onChanged: (MobileEdgeTtsSettings) -> Unit,
    onRetryCatalog: () -> Unit,
    onPreviewVoice: (String, String) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Card {
            Column(
                modifier = Modifier.padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text(
                    text = locale.ttsEdgeTitle,
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.SemiBold,
                )
                Text(
                    text = locale.ttsEdgeDesc,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                SliderField(
                    label = locale.ttsPitchLabel,
                    value = settings.pitch.toFloat(),
                    valueRange = -50f..50f,
                    valueLabel = "${settings.pitch} Hz",
                    onValueChange = { onChanged(settings.copy(pitch = it.toInt())) },
                )
                SliderField(
                    label = locale.ttsRateLabel,
                    value = settings.rate.toFloat(),
                    valueRange = -50f..100f,
                    valueLabel = "${settings.rate}%",
                    onValueChange = { onChanged(settings.copy(rate = it.toInt())) },
                )
                SliderField(
                    label = locale.ttsVolumeLabel,
                    value = settings.volume.toFloat(),
                    valueRange = -50f..50f,
                    valueLabel = "${settings.volume}%",
                    onValueChange = { onChanged(settings.copy(volume = it.toInt())) },
                )
            }
        }

        EdgeVoiceRoutingCard(
            settings = settings,
            locale = locale,
            catalogState = catalogState,
            onChanged = onChanged,
            onRetryCatalog = onRetryCatalog,
            onPreviewVoice = onPreviewVoice,
        )
    }
}

@Composable
private fun EdgeVoiceRoutingCard(
    settings: MobileEdgeTtsSettings,
    locale: MobileLocaleText,
    catalogState: EdgeVoiceCatalogState,
    onChanged: (MobileEdgeTtsSettings) -> Unit,
    onRetryCatalog: () -> Unit,
    onPreviewVoice: (String, String) -> Unit,
) {
    var addMenuExpanded by remember { mutableStateOf(false) }

    Card {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(
                text = locale.ttsVoicePerLanguageLabel,
                style = MaterialTheme.typography.labelLarge,
                fontWeight = FontWeight.SemiBold,
            )

            when {
                catalogState.loading -> Box(
                    modifier = Modifier.fillMaxWidth(),
                    contentAlignment = Alignment.Center,
                ) {
                    ContainedLoadingIndicator()
                }

                catalogState.errorMessage != null -> Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Text(
                        text = locale.ttsFailedLoadVoices(catalogState.errorMessage.orEmpty()),
                        modifier = Modifier.weight(1f),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.error,
                    )
                    OutlinedButton(onClick = onRetryCatalog) {
                        Text(locale.ttsRetryLabel)
                    }
                }
            }

            settings.voiceConfigs.forEachIndexed { index, config ->
                EdgeVoiceConfigRow(
                    config = config,
                    availableVoices = catalogState.byLanguage[config.languageCode.lowercase()].orEmpty().map { it.shortName },
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
                    locale = locale,
                    onPreview = { onPreviewVoice(config.languageCode, config.voiceName) },
                )
            }

            Box {
                OutlinedButton(onClick = { addMenuExpanded = true }) {
                    Text(locale.ttsAddLanguageLabel)
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
                                    val defaultVoice = catalogState.byLanguage[option.code.lowercase()]
                                        ?.firstOrNull()
                                        ?.shortName
                                        ?: MobileTtsCatalog.edgeVoiceSuggestions(option.code).firstOrNull()
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
                    text = locale.ttsResetToDefaultsLabel,
                    modifier = Modifier.padding(start = 6.dp),
                )
            }

        }
    }
}

@Composable
private fun EdgeVoiceConfigRow(
    config: MobileEdgeTtsVoiceConfig,
    availableVoices: List<String>,
    locale: MobileLocaleText,
    onConfigChanged: (MobileEdgeTtsVoiceConfig) -> Unit,
    onRemove: () -> Unit,
    onPreview: () -> Unit,
) {
    var suggestionsExpanded by remember(config.languageCode, config.voiceName) { mutableStateOf(false) }
    val suggestions = if (availableVoices.isNotEmpty()) {
        availableVoices
    } else {
        MobileTtsCatalog.edgeVoiceSuggestions(config.languageCode)
    }

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
                        label = { Text(locale.ttsVoiceLabel) },
                    )
                    if (suggestions.isNotEmpty()) {
                        Box {
                            IconButton(onClick = { suggestionsExpanded = true }) {
                                Icon(Icons.Rounded.ArrowDropDown, contentDescription = locale.ttsVoiceLabel)
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
                    IconButton(onClick = onPreview) {
                        Icon(Icons.AutoMirrored.Rounded.VolumeUp, contentDescription = locale.ttsVoiceLabel)
                    }
                    IconButton(onClick = onRemove) {
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
                    label = { Text(locale.ttsVoiceLabel) },
                )
                if (suggestions.isNotEmpty()) {
                    Box {
                        IconButton(onClick = { suggestionsExpanded = true }) {
                            Icon(Icons.Rounded.ArrowDropDown, contentDescription = locale.ttsVoiceLabel)
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
                IconButton(onClick = onPreview) {
                    Icon(Icons.AutoMirrored.Rounded.VolumeUp, contentDescription = locale.ttsVoiceLabel)
                }
                IconButton(onClick = onRemove) {
                    Icon(Icons.Rounded.Close, contentDescription = locale.removeLabel)
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
