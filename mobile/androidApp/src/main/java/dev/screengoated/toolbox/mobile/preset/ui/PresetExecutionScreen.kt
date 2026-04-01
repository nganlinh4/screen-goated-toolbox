@file:OptIn(ExperimentalMaterial3ExpressiveApi::class, ExperimentalMaterial3Api::class)

package dev.screengoated.toolbox.mobile.preset.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.ui.res.painterResource
import dev.screengoated.toolbox.mobile.R
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearWavyProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.preset.PresetExecutionState
import dev.screengoated.toolbox.mobile.preset.PresetRepository
import dev.screengoated.toolbox.mobile.shared.preset.Preset
import dev.screengoated.toolbox.mobile.shared.preset.PresetInput
import dev.screengoated.toolbox.mobile.shared.preset.PresetType

@Composable
fun PresetExecutionScreen(
    preset: Preset,
    presetRepository: PresetRepository,
    lang: String,
    onBack: () -> Unit,
) {
    val state by presetRepository.executionState.collectAsState()

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(preset.name(lang)) },
                navigationIcon = {
                    IconButton(onClick = {
                        presetRepository.resetState()
                        onBack()
                    }) {
                        Icon(painterResource(R.drawable.ms_arrow_back), contentDescription = null)
                    }
                },
            )
        },
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(horizontal = 16.dp),
        ) {
            when (preset.presetType) {
                PresetType.TEXT_INPUT -> TextInputPresetContent(
                    preset = preset,
                    state = state,
                    onExecute = { text ->
                        presetRepository.executePreset(preset, PresetInput.Text(text))
                    },
                    onCancel = { presetRepository.cancelExecution() },
                )
                PresetType.TEXT_SELECT -> TextSelectPresetContent(
                    preset = preset,
                    state = state,
                    onExecute = { text ->
                        presetRepository.executePreset(preset, PresetInput.Text(text))
                    },
                    onCancel = { presetRepository.cancelExecution() },
                )
                else -> {
                    // Phase 2+ features
                    Box(
                        modifier = Modifier.fillMaxSize(),
                        contentAlignment = Alignment.Center,
                    ) {
                        Text(
                            "Coming soon — requires ${preset.presetType.name} capture",
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun TextInputPresetContent(
    preset: Preset,
    state: PresetExecutionState,
    onExecute: (String) -> Unit,
    onCancel: () -> Unit,
) {
    var inputText by remember { mutableStateOf("") }
    val clipboard = LocalClipboardManager.current

    Column(
        modifier = Modifier.fillMaxSize(),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        // Input field
        OutlinedTextField(
            value = inputText,
            onValueChange = { inputText = it },
            label = { Text(if (preset.promptMode == "dynamic") "Type your question..." else "Input") },
            modifier = Modifier.fillMaxWidth(),
            shape = MaterialTheme.shapes.large,
            minLines = 3,
            maxLines = 6,
            trailingIcon = {
                IconButton(
                    onClick = { onExecute(inputText) },
                    enabled = inputText.isNotBlank() && !state.isExecuting,
                ) {
                    Icon(painterResource(R.drawable.ms_send), contentDescription = "Send")
                }
            },
        )

        // Result area
        ResultArea(state = state, onCancel = onCancel, clipboard = clipboard)
    }
}

@Composable
private fun TextSelectPresetContent(
    preset: Preset,
    state: PresetExecutionState,
    onExecute: (String) -> Unit,
    onCancel: () -> Unit,
) {
    var inputText by remember { mutableStateOf("") }
    val clipboard = LocalClipboardManager.current

    Column(
        modifier = Modifier.fillMaxSize(),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        // Paste text area
        OutlinedTextField(
            value = inputText,
            onValueChange = { inputText = it },
            label = { Text("Paste or type text to process") },
            modifier = Modifier.fillMaxWidth(),
            shape = MaterialTheme.shapes.large,
            minLines = 3,
            maxLines = 8,
        )

        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            FilledTonalButton(
                onClick = {
                    clipboard.getText()?.text?.let { inputText = it }
                },
            ) {
                Text("Paste")
            }
            FilledTonalButton(
                onClick = { onExecute(inputText) },
                enabled = inputText.isNotBlank() && !state.isExecuting,
            ) {
                Icon(painterResource(R.drawable.ms_send), contentDescription = null, Modifier.size(16.dp))
                Spacer(Modifier.width(4.dp))
                Text("Process")
            }
        }

        // Result area
        ResultArea(state = state, onCancel = onCancel, clipboard = clipboard)
    }
}

@Composable
private fun ResultArea(
    state: PresetExecutionState,
    onCancel: () -> Unit,
    clipboard: androidx.compose.ui.platform.ClipboardManager,
) {
    val combinedText = state.resultWindows.joinToString(separator = "\n\n") { it.markdownText }

    // Progress
    if (state.isExecuting) {
        LinearWavyProgressIndicator(
            modifier = Modifier.fillMaxWidth().height(5.dp),
        )
    }

    // Error
    if (state.error != null) {
        Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.errorContainer)) {
            Text(
                state.error,
                modifier = Modifier.padding(12.dp),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onErrorContainer,
            )
        }
    }

    // Streaming result
    if (combinedText.isNotBlank()) {
        Card(
            modifier = Modifier.fillMaxWidth(),
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
        ) {
            Column(modifier = Modifier.padding(12.dp)) {
                Text(
                    combinedText,
                    style = MaterialTheme.typography.bodyMedium,
                    modifier = Modifier
                        .fillMaxWidth()
                        .verticalScroll(rememberScrollState()),
                )
                Spacer(Modifier.height(8.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    if (state.isExecuting) {
                        FilledTonalButton(onClick = onCancel) {
                            Icon(painterResource(R.drawable.ms_stop), contentDescription = null, Modifier.size(16.dp))
                            Spacer(Modifier.width(4.dp))
                            Text("Stop")
                        }
                    }
                    if (state.isComplete || combinedText.isNotBlank()) {
                        FilledTonalButton(onClick = {
                            clipboard.setText(AnnotatedString(combinedText))
                        }) {
                            Icon(painterResource(R.drawable.ms_content_copy), contentDescription = null, Modifier.size(16.dp))
                            Spacer(Modifier.width(4.dp))
                            Text("Copy")
                        }
                    }
                }
            }
        }
    }
}
