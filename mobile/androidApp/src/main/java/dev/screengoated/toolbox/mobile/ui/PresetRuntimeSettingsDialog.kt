package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.preset.PresetModelPriorityChains
import dev.screengoated.toolbox.mobile.preset.PresetProviderSettings
import dev.screengoated.toolbox.mobile.preset.PresetRuntimeSettings
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

@Composable
fun PresetRuntimeSettingsDialog(
    settings: PresetRuntimeSettings,
    locale: MobileLocaleText,
    onDismiss: () -> Unit,
    onSave: (PresetRuntimeSettings) -> Unit,
) {
    var providerSettings by remember(settings) { mutableStateOf(settings.providerSettings) }
    var imageToText by remember(settings) { mutableStateOf(settings.modelPriorityChains.imageToText.joinToString(", ")) }
    var textToText by remember(settings) { mutableStateOf(settings.modelPriorityChains.textToText.joinToString(", ")) }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = {
            Text(
                text = locale.presetRuntimeTitle,
                style = MaterialTheme.typography.titleLarge,
            )
        },
        text = {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text(
                    text = locale.presetRuntimeDescription,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                ProviderToggleCard(
                    title = "Gemini",
                    checked = providerSettings.useGemini,
                    onCheckedChange = { providerSettings = providerSettings.copy(useGemini = it) },
                )
                ProviderToggleCard(
                    title = "Cerebras",
                    checked = providerSettings.useCerebras,
                    onCheckedChange = { providerSettings = providerSettings.copy(useCerebras = it) },
                )
                ProviderToggleCard(
                    title = "Groq",
                    checked = providerSettings.useGroq,
                    onCheckedChange = { providerSettings = providerSettings.copy(useGroq = it) },
                )
                ProviderToggleCard(
                    title = "OpenRouter",
                    checked = providerSettings.useOpenRouter,
                    onCheckedChange = { providerSettings = providerSettings.copy(useOpenRouter = it) },
                )
                ProviderToggleCard(
                    title = "Ollama",
                    checked = providerSettings.useOllama,
                    onCheckedChange = { providerSettings = providerSettings.copy(useOllama = it) },
                )
                OutlinedTextField(
                    modifier = Modifier.fillMaxWidth(),
                    value = imageToText,
                    onValueChange = { imageToText = it },
                    label = { Text(locale.presetRuntimeImageChainLabel) },
                    supportingText = { Text(locale.presetRuntimeChainHint) },
                    minLines = 2,
                )
                OutlinedTextField(
                    modifier = Modifier.fillMaxWidth(),
                    value = textToText,
                    onValueChange = { textToText = it },
                    label = { Text(locale.presetRuntimeTextChainLabel) },
                    supportingText = { Text(locale.presetRuntimeChainHint) },
                    minLines = 2,
                )
            }
        },
        confirmButton = {
            TextButton(
                onClick = {
                    onSave(
                        PresetRuntimeSettings(
                            providerSettings = providerSettings,
                            modelPriorityChains = PresetModelPriorityChains(
                                imageToText = parseModelChain(imageToText, settings.modelPriorityChains.imageToText),
                                textToText = parseModelChain(textToText, settings.modelPriorityChains.textToText),
                            ),
                        ),
                    )
                },
            ) {
                Text(locale.presetRuntimeSave)
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text(locale.presetRuntimeCancel)
            }
        },
    )
}

@Composable
private fun ProviderToggleCard(
    title: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
) {
    Card {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 14.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text(
                text = title,
                style = MaterialTheme.typography.titleSmall,
            )
            Switch(
                checked = checked,
                onCheckedChange = onCheckedChange,
            )
        }
    }
}

private fun parseModelChain(
    raw: String,
    fallback: List<String>,
): List<String> {
    val parsed = raw.split(",")
        .map { it.trim() }
        .filter { it.isNotBlank() }
        .distinct()
    return if (parsed.isEmpty()) fallback else parsed
}
