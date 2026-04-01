@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui.ttssettings

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.ui.res.painterResource
import dev.screengoated.toolbox.mobile.R
import androidx.compose.ui.draw.clip
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
import dev.screengoated.toolbox.mobile.ui.ExpressiveDialogSectionCard
import dev.screengoated.toolbox.mobile.ui.UtilityActionButton
import dev.screengoated.toolbox.mobile.ui.UtilityHeaderRow
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
        ExpressiveDialogSectionCard(accent = MaterialTheme.colorScheme.primary) {
            UtilityHeaderRow(
                icon = R.drawable.ms_settings_voice,
                title = locale.ttsEdgeTitle,
                accent = MaterialTheme.colorScheme.primary,
                supporting = locale.ttsEdgeDesc,
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

    ExpressiveDialogSectionCard(accent = MaterialTheme.colorScheme.secondary) {
        UtilityHeaderRow(
            icon = R.drawable.ms_settings_voice,
            title = locale.ttsVoicePerLanguageLabel,
            accent = MaterialTheme.colorScheme.secondary,
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
                UtilityActionButton(
                    text = locale.ttsRetryLabel,
                    accent = MaterialTheme.colorScheme.error,
                    onClick = onRetryCatalog,
                ) {
                    Icon(painterResource(R.drawable.ms_refresh), contentDescription = null, tint = MaterialTheme.colorScheme.error)
                }
            }
        }

        settings.voiceConfigs.forEachIndexed { index, config ->
            EdgeVoiceConfigRow(
                config = config,
                availableVoices = catalogState.byLanguage[config.languageCode.lowercase()].orEmpty().map { it.shortName },
                accent = MaterialTheme.colorScheme.secondary,
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

        BoxWithConstraints {
            val stackedActions = maxWidth < 520.dp

            if (stackedActions) {
                Column(
                    modifier = Modifier.fillMaxWidth(),
                    verticalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    EdgeRoutingAddButton(
                        modifier = Modifier.fillMaxWidth(),
                        locale = locale,
                        addMenuExpanded = addMenuExpanded,
                        onAddMenuExpandedChanged = { addMenuExpanded = it },
                        settings = settings,
                        catalogState = catalogState,
                        onChanged = onChanged,
                    )
                    UtilityActionButton(
                        text = locale.resetDefaultsAction,
                        accent = MaterialTheme.colorScheme.primary,
                        onClick = { onChanged(MobileEdgeTtsSettings()) },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Icon(painterResource(R.drawable.ms_refresh), contentDescription = null, tint = MaterialTheme.colorScheme.primary)
                    }
                }
            } else {
                FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                    verticalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    EdgeRoutingAddButton(
                        locale = locale,
                        addMenuExpanded = addMenuExpanded,
                        onAddMenuExpandedChanged = { addMenuExpanded = it },
                        settings = settings,
                        catalogState = catalogState,
                        onChanged = onChanged,
                    )
                    UtilityActionButton(
                        text = locale.resetDefaultsAction,
                        accent = MaterialTheme.colorScheme.primary,
                        onClick = { onChanged(MobileEdgeTtsSettings()) },
                    ) {
                        Icon(painterResource(R.drawable.ms_refresh), contentDescription = null, tint = MaterialTheme.colorScheme.primary)
                    }
                }
            }
        }
    }
}

@Composable
private fun EdgeRoutingAddButton(
    locale: MobileLocaleText,
    addMenuExpanded: Boolean,
    onAddMenuExpandedChanged: (Boolean) -> Unit,
    settings: MobileEdgeTtsSettings,
    catalogState: EdgeVoiceCatalogState,
    onChanged: (MobileEdgeTtsSettings) -> Unit,
    modifier: Modifier = Modifier,
) {
    Box(modifier = modifier) {
        UtilityActionButton(
            text = locale.ttsAddLanguageLabel,
            accent = MaterialTheme.colorScheme.secondary,
            onClick = { onAddMenuExpandedChanged(true) },
            modifier = modifier,
        )
        DropdownMenu(
            expanded = addMenuExpanded,
            onDismissRequest = { onAddMenuExpandedChanged(false) },
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
                            onAddMenuExpandedChanged(false)
                        },
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
    accent: androidx.compose.ui.graphics.Color,
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
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(MaterialTheme.shapes.medium)
                    .background(accent.copy(alpha = 0.08f))
                    .padding(12.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                ConditionLanguageLabel(config.languageName)
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    EdgeVoiceSelector(
                        currentVoice = config.voiceName,
                        suggestions = suggestions,
                        locale = locale,
                        onVoiceSelected = { onConfigChanged(config.copy(voiceName = it)) },
                        expanded = suggestionsExpanded,
                        onExpandedChanged = { suggestionsExpanded = it },
                        modifier = Modifier.weight(1f),
                    )
                    IconButton(onClick = onPreview) {
                        Icon(painterResource(R.drawable.ms_volume_up), contentDescription = locale.ttsVoiceLabel)
                    }
                    IconButton(onClick = onRemove) {
                        Icon(painterResource(R.drawable.ms_close), contentDescription = locale.removeLabel)
                    }
                }
            }
        } else {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(MaterialTheme.shapes.medium)
                    .background(accent.copy(alpha = 0.08f))
                    .padding(horizontal = 12.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                ConditionLanguageLabel(config.languageName, Modifier.width(138.dp))
                Icon(
                    painterResource(R.drawable.ms_arrow_forward),
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                EdgeVoiceSelector(
                    currentVoice = config.voiceName,
                    suggestions = suggestions,
                    locale = locale,
                    onVoiceSelected = { onConfigChanged(config.copy(voiceName = it)) },
                    expanded = suggestionsExpanded,
                    onExpandedChanged = { suggestionsExpanded = it },
                    modifier = Modifier.weight(1f),
                )
                IconButton(onClick = onPreview) {
                    Icon(painterResource(R.drawable.ms_volume_up), contentDescription = locale.ttsVoiceLabel)
                }
                IconButton(onClick = onRemove) {
                    Icon(painterResource(R.drawable.ms_close), contentDescription = locale.removeLabel)
                }
            }
        }
    }
}

@Composable
private fun EdgeVoiceSelector(
    currentVoice: String,
    suggestions: List<String>,
    locale: MobileLocaleText,
    onVoiceSelected: (String) -> Unit,
    expanded: Boolean,
    onExpandedChanged: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
) {
    Box(modifier = modifier) {
        OutlinedButton(
            onClick = { onExpandedChanged(true) },
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = 56.dp),
        ) {
            Text(
                text = currentVoice,
                modifier = Modifier.weight(1f),
                maxLines = 1,
            )
            Icon(painterResource(R.drawable.ms_arrow_drop_down), contentDescription = locale.ttsVoiceLabel)
        }
        DropdownMenu(
            expanded = expanded,
            onDismissRequest = { onExpandedChanged(false) },
        ) {
            suggestions.forEach { voice ->
                DropdownMenuItem(
                    text = { Text(voice) },
                    onClick = {
                        onVoiceSelected(voice)
                        onExpandedChanged(false)
                    },
                )
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
