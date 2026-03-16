@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
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
import androidx.compose.material.icons.rounded.Visibility
import androidx.compose.material.icons.rounded.VisibilityOff
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ButtonGroupDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.ToggleButton
import androidx.compose.material3.ToggleButtonDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.foundation.clickable
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionState
import dev.screengoated.toolbox.mobile.shared.live.SessionPhase
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

private data class ProviderDef(
    val label: String,
    val icon: ImageVector,
    val keyLabel: String,
    val getKeyUrl: String? = null,
    val getKeyLabel: String? = null,
)

@Composable
internal fun CredentialsCard(
    apiKey: String,
    cerebrasApiKey: String,
    groqApiKey: String,
    openRouterApiKey: String,
    ollamaUrl: String,
    locale: MobileLocaleText,
    onApiKeyChanged: (String) -> Unit,
    onCerebrasApiKeyChanged: (String) -> Unit,
    onGroqApiKeyChanged: (String) -> Unit,
    onOpenRouterApiKeyChanged: (String) -> Unit,
    onOllamaUrlChanged: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val uriHandler = LocalUriHandler.current
    val providers = listOf(
        ProviderDef("Gemini", Icons.Rounded.AutoAwesome, locale.geminiKeyLabel, "https://aistudio.google.com/app/apikey", locale.geminiGetKeyLink),
        ProviderDef("Cerebras", Icons.Rounded.LocalFireDepartment, locale.cerebrasKeyLabel, "https://cloud.cerebras.ai/", locale.cerebrasGetKeyLink),
        ProviderDef("Groq", Icons.Rounded.Bolt, locale.groqKeyLabel, "https://console.groq.com/keys", locale.groqGetKeyLink),
        ProviderDef("OpenRouter", Icons.Rounded.Public, locale.openRouterKeyLabel, "https://openrouter.ai/settings/keys", locale.openRouterGetKeyLink),
        ProviderDef("Ollama", Icons.Rounded.Computer, locale.ollamaUrlLabel),
    )
    val enabledState = remember { mutableStateListOf(true, true, true, false, false) }
    val visibleState = remember { mutableStateListOf(false, false, false, false, false) }

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
            verticalArrangement = Arrangement.spacedBy(10.dp),
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

            val isLandscape = LocalConfiguration.current.orientation ==
                android.content.res.Configuration.ORIENTATION_LANDSCAPE

            data class FieldEntry(
                val index: Int,
                val value: String,
                val onValueChange: (String) -> Unit,
                val isPassword: Boolean = true,
            )

            val fields = listOf(
                FieldEntry(0, apiKey, onApiKeyChanged),
                FieldEntry(1, cerebrasApiKey, onCerebrasApiKeyChanged),
                FieldEntry(2, groqApiKey, onGroqApiKeyChanged),
                FieldEntry(3, openRouterApiKey, onOpenRouterApiKeyChanged),
                FieldEntry(4, ollamaUrl, onOllamaUrlChanged, isPassword = false),
            )
            @Composable
            fun ApiKeyFieldContent(
                entry: FieldEntry,
                fieldModifier: Modifier = Modifier,
            ) {
                Column(
                    modifier = fieldModifier,
                    verticalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    OutlinedTextField(
                        modifier = Modifier.fillMaxWidth(),
                        value = entry.value,
                        onValueChange = entry.onValueChange,
                        label = { Text(providers[entry.index].keyLabel, style = MaterialTheme.typography.labelSmall) },
                        singleLine = true,
                        textStyle = MaterialTheme.typography.bodySmall,
                        visualTransformation = if (entry.isPassword && !visibleState[entry.index]) {
                            PasswordVisualTransformation()
                        } else {
                            VisualTransformation.None
                        },
                        trailingIcon = if (entry.isPassword) {
                            {
                                IconButton(onClick = { visibleState[entry.index] = !visibleState[entry.index] }) {
                                    Icon(
                                        if (visibleState[entry.index]) Icons.Rounded.VisibilityOff
                                        else Icons.Rounded.Visibility,
                                        contentDescription = null,
                                        modifier = Modifier.size(18.dp),
                                    )
                                }
                            }
                        } else null,
                        shape = MaterialTheme.shapes.medium,
                    )
                    val provider = providers[entry.index]
                    if (provider.getKeyUrl != null && provider.getKeyLabel != null) {
                        Text(
                            text = provider.getKeyLabel,
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.primary,
                            modifier = Modifier
                                .clickable { uriHandler.openUri(provider.getKeyUrl) }
                                .padding(start = 4.dp),
                        )
                    }
                }
            }

            if (isLandscape) {
                // 2-column grid in landscape to save vertical space
                fields.chunked(2).forEach { pair ->
                    val anyVisible = pair.any { enabledState[it.index] }
                    AnimatedVisibility(visible = anyVisible) {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(10.dp),
                        ) {
                            pair.forEach { entry ->
                                AnimatedVisibility(
                                    visible = enabledState[entry.index],
                                    modifier = Modifier.weight(1f),
                                ) {
                                    ApiKeyFieldContent(entry = entry)
                                }
                            }
                            if (pair.size == 1) {
                                Spacer(Modifier.weight(1f))
                            }
                        }
                    }
                }
            } else {
                fields.forEach { entry ->
                    AnimatedVisibility(visible = enabledState[entry.index]) {
                        ApiKeyFieldContent(entry)
                    }
                }
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
