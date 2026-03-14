@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.AutoAwesome
import androidx.compose.material.icons.rounded.Bolt
import androidx.compose.material.icons.rounded.Cloud
import androidx.compose.material.icons.rounded.Computer
import androidx.compose.material.icons.rounded.GraphicEq
import androidx.compose.material.icons.rounded.Key
import androidx.compose.material.icons.rounded.LocalFireDepartment
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.Public
import androidx.compose.material.icons.rounded.Stop
import androidx.compose.material.icons.rounded.Translate
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ButtonGroupDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.ToggleButton
import androidx.compose.material3.ToggleButtonDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionState
import dev.screengoated.toolbox.mobile.shared.live.SessionPhase
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

private data class ProviderDef(
    val label: String,
    val icon: ImageVector,
    val keyLabel: String,
)

@Composable
internal fun CredentialsCard(
    apiKey: String,
    cerebrasApiKey: String,
    locale: MobileLocaleText,
    onApiKeyChanged: (String) -> Unit,
    onCerebrasApiKeyChanged: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val providers = listOf(
        ProviderDef("Gemini", Icons.Rounded.AutoAwesome, locale.geminiKeyLabel),
        ProviderDef("Cerebras", Icons.Rounded.LocalFireDepartment, locale.cerebrasKeyLabel),
        ProviderDef("Groq", Icons.Rounded.Bolt, "Groq key"),
        ProviderDef("OpenRouter", Icons.Rounded.Public, "OpenRouter key"),
        ProviderDef("Ollama", Icons.Rounded.Computer, "Ollama URL"),
    )
    val enabledState = remember { mutableStateListOf(true, true, false, false, false) }
    // Local state for providers not yet wired to ViewModel
    var groqKey by remember { mutableStateOf("") }
    var openRouterKey by remember { mutableStateOf("") }
    var ollamaUrl by remember { mutableStateOf("http://localhost:11434") }

    Card(
        modifier = modifier,
        shape = MaterialTheme.shapes.extraLarge,
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
        ),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(ShellSpacing.innerPad),
            verticalArrangement = Arrangement.spacedBy(ShellSpacing.itemGap),
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(ShellSpacing.itemGap),
            ) {
                GradientMaskedIcon(
                    Icons.Rounded.Key,
                    Brush.linearGradient(
                        listOf(
                            MaterialTheme.colorScheme.primary,
                            MaterialTheme.colorScheme.tertiary,
                        ),
                    ),
                    modifier = Modifier.size(22.dp),
                )
                Text(
                    text = locale.shellCredentialsTitle,
                    style = MaterialTheme.typography.titleMedium,
                )
            }
            // Provider toggle chips — compact connected buttons
            FlowRow(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(ButtonGroupDefaults.ConnectedSpaceBetween),
                verticalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                providers.forEachIndexed { index, provider ->
                    ToggleButton(
                        checked = enabledState[index],
                        onCheckedChange = { enabledState[index] = it },
                        shapes = when (index) {
                            0 -> ButtonGroupDefaults.connectedLeadingButtonShapes()
                            providers.lastIndex -> ButtonGroupDefaults.connectedTrailingButtonShapes()
                            else -> ButtonGroupDefaults.connectedMiddleButtonShapes()
                        },
                        modifier = Modifier.semantics { role = Role.Checkbox },
                    ) {
                        Icon(
                            provider.icon,
                            contentDescription = null,
                            modifier = Modifier.size(16.dp),
                        )
                        Spacer(Modifier.size(ToggleButtonDefaults.IconSpacing))
                        Text(
                            provider.label,
                            style = MaterialTheme.typography.labelSmall,
                        )
                    }
                }
            }
            // Gemini key
            AnimatedVisibility(visible = enabledState[0]) {
                OutlinedTextField(
                    modifier = Modifier.fillMaxWidth(),
                    value = apiKey,
                    onValueChange = onApiKeyChanged,
                    label = { Text(providers[0].keyLabel) },
                    singleLine = true,
                    visualTransformation = PasswordVisualTransformation(),
                    shape = MaterialTheme.shapes.large,
                )
            }
            // Cerebras key
            AnimatedVisibility(visible = enabledState[1]) {
                OutlinedTextField(
                    modifier = Modifier.fillMaxWidth(),
                    value = cerebrasApiKey,
                    onValueChange = onCerebrasApiKeyChanged,
                    label = { Text(providers[1].keyLabel) },
                    singleLine = true,
                    visualTransformation = PasswordVisualTransformation(),
                    shape = MaterialTheme.shapes.large,
                )
            }
            // Groq key
            AnimatedVisibility(visible = enabledState[2]) {
                OutlinedTextField(
                    modifier = Modifier.fillMaxWidth(),
                    value = groqKey,
                    onValueChange = { groqKey = it },
                    label = { Text(providers[2].keyLabel) },
                    singleLine = true,
                    visualTransformation = PasswordVisualTransformation(),
                    shape = MaterialTheme.shapes.large,
                )
            }
            // OpenRouter key
            AnimatedVisibility(visible = enabledState[3]) {
                OutlinedTextField(
                    modifier = Modifier.fillMaxWidth(),
                    value = openRouterKey,
                    onValueChange = { openRouterKey = it },
                    label = { Text(providers[3].keyLabel) },
                    singleLine = true,
                    visualTransformation = PasswordVisualTransformation(),
                    shape = MaterialTheme.shapes.large,
                )
            }
            // Ollama URL
            AnimatedVisibility(visible = enabledState[4]) {
                OutlinedTextField(
                    modifier = Modifier.fillMaxWidth(),
                    value = ollamaUrl,
                    onValueChange = { ollamaUrl = it },
                    label = { Text(providers[4].keyLabel) },
                    singleLine = true,
                    shape = MaterialTheme.shapes.large,
                )
            }
        }
    }
}

@Composable
internal fun VoiceSettingsCard(
    globalTtsSettings: MobileGlobalTtsSettings,
    locale: MobileLocaleText,
    onVoiceSettingsClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Card(
        modifier = modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.extraLarge,
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
        ),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = ShellSpacing.innerPad, vertical = 14.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ShellSpacing.itemGap),
        ) {
            GradientMaskedIcon(
                Icons.Rounded.GraphicEq,
                Brush.linearGradient(
                    listOf(
                        MaterialTheme.colorScheme.secondary,
                        MaterialTheme.colorScheme.primary,
                    ),
                ),
                modifier = Modifier.size(22.dp),
            )
            Text(
                text = methodLabel(locale, globalTtsSettings.method),
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.weight(1f),
            )
            FilledTonalButton(
                onClick = onVoiceSettingsClick,
                shape = CircleShape,
            ) {
                Text(
                    text = locale.voiceSettingsButton,
                    style = MaterialTheme.typography.labelMediumEmphasized,
                )
            }
        }
    }
}

@Composable
internal fun LiveControlCard(
    state: LiveSessionState,
    locale: MobileLocaleText,
    onSessionToggle: () -> Unit,
    canToggle: Boolean,
) {
    val isRunning = state.phase in setOf(
        SessionPhase.STARTING,
        SessionPhase.LISTENING,
        SessionPhase.TRANSLATING,
    )
    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.extraLarge,
        colors = CardDefaults.cardColors(
            containerColor = if (isRunning) {
                MaterialTheme.colorScheme.primaryContainer
            } else {
                MaterialTheme.colorScheme.surfaceContainerLow
            },
        ),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = ShellSpacing.innerPad, vertical = 14.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ShellSpacing.itemGap),
        ) {
            GradientMaskedIcon(
                Icons.Rounded.Translate,
                if (isRunning) {
                    Brush.linearGradient(
                        listOf(
                            MaterialTheme.colorScheme.primary,
                            MaterialTheme.colorScheme.secondary,
                            MaterialTheme.colorScheme.tertiary,
                        ),
                    )
                } else {
                    Brush.linearGradient(
                        listOf(
                            MaterialTheme.colorScheme.tertiary,
                            MaterialTheme.colorScheme.primary,
                        ),
                    )
                },
                modifier = Modifier.size(24.dp),
            )
            Text(
                text = locale.shellLiveTitle,
                style = MaterialTheme.typography.titleMedium,
                color = if (isRunning) {
                    MaterialTheme.colorScheme.onPrimaryContainer
                } else {
                    MaterialTheme.colorScheme.onSurface
                },
                modifier = Modifier.weight(1f),
            )
            Button(
                onClick = onSessionToggle,
                enabled = canToggle,
                shape = CircleShape,
                colors = if (isRunning) {
                    ButtonDefaults.buttonColors(
                        containerColor = MaterialTheme.colorScheme.error,
                    )
                } else {
                    ButtonDefaults.buttonColors()
                },
            ) {
                Icon(
                    if (isRunning) Icons.Rounded.Stop else Icons.Rounded.PlayArrow,
                    contentDescription = null,
                    modifier = Modifier.size(18.dp),
                )
                Spacer(modifier = Modifier.padding(start = ButtonDefaults.IconSpacing))
                Text(
                    text = if (state.phase in setOf(SessionPhase.STOPPED, SessionPhase.IDLE, SessionPhase.ERROR, SessionPhase.AWAITING_PERMISSIONS)) locale.turnOn else locale.turnOff,
                    style = MaterialTheme.typography.labelLargeEmphasized,
                )
            }
        }
    }
}
