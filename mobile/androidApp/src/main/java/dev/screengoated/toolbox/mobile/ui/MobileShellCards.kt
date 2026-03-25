@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.AutoAwesome
import androidx.compose.material.icons.rounded.BarChart
import androidx.compose.material.icons.rounded.Bolt
import androidx.compose.material.icons.rounded.Computer
import androidx.compose.material.icons.rounded.Key
import androidx.compose.material.icons.rounded.LocalFireDepartment
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.Public
import androidx.compose.material.icons.rounded.RestartAlt
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material.icons.rounded.Stop
import androidx.compose.material.icons.rounded.Translate
import androidx.compose.material.icons.rounded.Visibility
import androidx.compose.material.icons.rounded.VisibilityOff
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialShapes
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.foundation.clickable
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.preset.PresetRuntimeSettings
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
    val providers = credentialsProviderOrder().map { provider ->
        when (provider) {
            CredentialsProviderId.GROQ -> ProviderDef(
                label = provider.label,
                icon = Icons.Rounded.Bolt,
                keyLabel = locale.groqKeyLabel,
                getKeyUrl = "https://console.groq.com/keys",
                getKeyLabel = locale.groqGetKeyLink,
            )
            CredentialsProviderId.CEREBRAS -> ProviderDef(
                label = provider.label,
                icon = Icons.Rounded.LocalFireDepartment,
                keyLabel = locale.cerebrasKeyLabel,
                getKeyUrl = "https://cloud.cerebras.ai/",
                getKeyLabel = locale.cerebrasGetKeyLink,
            )
            CredentialsProviderId.GEMINI -> ProviderDef(
                label = provider.label,
                icon = Icons.Rounded.AutoAwesome,
                keyLabel = locale.geminiKeyLabel,
                getKeyUrl = "https://aistudio.google.com/app/apikey",
                getKeyLabel = locale.geminiGetKeyLink,
            )
            CredentialsProviderId.OPEN_ROUTER -> ProviderDef(
                label = provider.label,
                icon = Icons.Rounded.Public,
                keyLabel = locale.openRouterKeyLabel,
                getKeyUrl = "https://openrouter.ai/settings/keys",
                getKeyLabel = locale.openRouterGetKeyLink,
            )
            CredentialsProviderId.OLLAMA -> ProviderDef(
                label = provider.label,
                icon = Icons.Rounded.Computer,
                keyLabel = locale.ollamaUrlLabel,
                getKeyUrl = "https://ollama.com/download",
                getKeyLabel = locale.ollamaLearnMoreLink,
            )
        }
    }
    val expandedState = remember { mutableStateListOf(true, true, true, false, false) }
    val visibleState = remember { mutableStateListOf(false, false, false, false, false) }
    val cardAccent = MaterialTheme.colorScheme.primary

    ExpressiveSettingsCard(
        modifier = modifier,
        accent = cardAccent,
    ) {
        Column(
            modifier = Modifier.fillMaxWidth(),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            ExpressiveSettingsHeader(
                title = locale.shellCredentialsTitle,
                icon = Icons.Rounded.Key,
                accent = cardAccent,
            )

            val isLandscape = LocalConfiguration.current.orientation ==
                android.content.res.Configuration.ORIENTATION_LANDSCAPE

            data class FieldEntry(
                val index: Int,
                val value: String,
                val onValueChange: (String) -> Unit,
                val isPassword: Boolean = true,
            )

            val fields = listOf(
                FieldEntry(0, groqApiKey, onGroqApiKeyChanged),
                FieldEntry(1, cerebrasApiKey, onCerebrasApiKeyChanged),
                FieldEntry(2, apiKey, onApiKeyChanged),
                FieldEntry(3, openRouterApiKey, onOpenRouterApiKeyChanged),
                FieldEntry(4, ollamaUrl, onOllamaUrlChanged, isPassword = false),
            )
            @Composable
            fun ApiKeyFieldContent(
                entry: FieldEntry,
                fieldModifier: Modifier = Modifier,
            ) {
                val provider = providers[entry.index]
                val accent = providerAccent(provider.label, MaterialTheme.colorScheme)
                val expanded = expandedState[entry.index]
                ExpressiveSettingsInsetCard(
                    accent = accent,
                    modifier = fieldModifier,
                    horizontalPadding = 10.dp,
                    verticalPadding = if (expanded) 8.dp else 6.dp,
                ) {
                    Column(
                        modifier = Modifier.fillMaxWidth(),
                        verticalArrangement = Arrangement.spacedBy(if (expanded) 6.dp else 4.dp),
                    ) {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(10.dp),
                        ) {
                            Box(
                                modifier = Modifier
                                    .weight(1f)
                                    .fillMaxWidth(),
                            ) {
                                Row(
                                    modifier = Modifier.align(Alignment.CenterStart),
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                                ) {
                                    MorphingShapeBadge(
                                        morphPair = ExpressiveMorphPair(
                                            MaterialShapes.Circle,
                                            MaterialShapes.Cookie4Sided,
                                        ),
                                        progress = 0.82f,
                                        containerColor = accent.copy(alpha = 0.18f),
                                        modifier = Modifier.size(28.dp),
                                    ) {
                                        Icon(
                                            imageVector = provider.icon,
                                            contentDescription = null,
                                            tint = accent,
                                            modifier = Modifier.size(14.dp),
                                        )
                                    }
                                    Text(
                                        text = provider.label,
                                        style = MaterialTheme.typography.labelMediumEmphasized,
                                        color = accent,
                                    )
                                }
                                if (expanded && provider.getKeyUrl != null && provider.getKeyLabel != null) {
                                    Text(
                                        text = provider.getKeyLabel,
                                        style = MaterialTheme.typography.labelSmall,
                                        color = accent,
                                        textAlign = TextAlign.Center,
                                        modifier = Modifier
                                            .align(Alignment.Center)
                                            .clickable { uriHandler.openUri(provider.getKeyUrl) },
                                    )
                                }
                            }
                            ExpressiveProviderExpandSwitch(
                                checked = expanded,
                                accent = accent,
                                onCheckedChange = { expandedState[entry.index] = it },
                                modifier = Modifier.semantics { role = Role.Switch },
                            )
                        }

                        AnimatedVisibility(visible = expanded) {
                            OutlinedTextField(
                                modifier = Modifier.fillMaxWidth(),
                                value = entry.value,
                                onValueChange = entry.onValueChange,
                                label = { Text(provider.keyLabel, style = MaterialTheme.typography.labelSmall) },
                                singleLine = true,
                                textStyle = MaterialTheme.typography.bodySmall,
                                visualTransformation = if (entry.isPassword && !visibleState[entry.index]) {
                                    PasswordVisualTransformation()
                                } else {
                                    VisualTransformation.None
                                },
                                trailingIcon = if (entry.isPassword) {
                                    {
                                        MorphingVisibilityToggleButton(
                                            visible = visibleState[entry.index],
                                            accent = accent,
                                            onClick = { visibleState[entry.index] = !visibleState[entry.index] },
                                            modifier = Modifier.size(36.dp),
                                        )
                                    }
                                } else null,
                                shape = MaterialTheme.shapes.large,
                                colors = OutlinedTextFieldDefaults.colors(
                                    focusedContainerColor = accent.copy(alpha = 0.1f),
                                    unfocusedContainerColor = accent.copy(alpha = 0.065f),
                                    disabledContainerColor = accent.copy(alpha = 0.05f),
                                    focusedBorderColor = accent.copy(alpha = 0.75f),
                                    unfocusedBorderColor = accent.copy(alpha = 0.34f),
                                    cursorColor = accent,
                                ),
                            )
                        }
                    }
                }
            }

            if (isLandscape) {
                val cells = buildList<List<FieldEntry>> {
                    var index = 0
                    while (index < fields.size) {
                        val current = fields[index]
                        val currentExpanded = expandedState[current.index]
                        val next = fields.getOrNull(index + 1)
                        if (!currentExpanded && next != null && !expandedState[next.index]) {
                            add(listOf(current, next))
                            index += 2
                        } else {
                            add(listOf(current))
                            index += 1
                        }
                    }
                }
                cells.chunked(2).forEach { pair ->
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        pair.forEach { cell ->
                            Box(modifier = Modifier.weight(1f)) {
                                if (cell.size == 1) {
                                    ApiKeyFieldContent(entry = cell.first())
                                } else {
                                    Column(
                                        verticalArrangement = Arrangement.spacedBy(8.dp),
                                    ) {
                                        cell.forEach { entry ->
                                            ApiKeyFieldContent(entry = entry)
                                        }
                                    }
                                }
                            }
                        }
                        if (pair.size == 1) {
                            Spacer(Modifier.weight(1f))
                        }
                    }
                }
            } else {
                fields.forEach { entry ->
                    ApiKeyFieldContent(entry)
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
    val accent = MaterialTheme.colorScheme.secondary
    val methodDescription = methodLabel(locale, globalTtsSettings.method)
    ExpressiveSettingsCard(
        modifier = modifier.fillMaxWidth(),
        accent = accent,
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .semantics { stateDescription = methodDescription },
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ShellSpacing.itemGap),
        ) {
            Text(
                text = methodDescription,
                style = MaterialTheme.typography.titleMedium,
                color = accent,
            )
            Spacer(modifier = Modifier.weight(1f))
            ExpressiveSettingsButton(
                text = locale.voiceSettingsButton,
                onClick = onVoiceSettingsClick,
                accent = accent,
            )
        }
    }
}

@Composable
internal fun PresetRuntimeCard(
    settings: PresetRuntimeSettings,
    locale: MobileLocaleText,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Card(
        modifier = modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.large,
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
                Icons.Rounded.Settings,
                Brush.linearGradient(
                    listOf(
                        MaterialTheme.colorScheme.tertiary,
                        MaterialTheme.colorScheme.primary,
                    ),
                ),
                modifier = Modifier.size(22.dp),
            )
            Text(
                text = locale.presetRuntimeButton,
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.weight(1f),
            )
            FilledTonalButton(
                onClick = onClick,
                shape = CircleShape,
            ) {
                Text(
                    text = locale.presetRuntimeSettingsAction,
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
        shape = MaterialTheme.shapes.large,
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

@Composable
internal fun UsageStatsCard(
    locale: MobileLocaleText,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Card(
        modifier = modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.large,
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
                Icons.Rounded.BarChart,
                Brush.linearGradient(
                    listOf(
                        MaterialTheme.colorScheme.primary,
                        MaterialTheme.colorScheme.secondary,
                    ),
                ),
                modifier = Modifier.size(22.dp),
            )
            Text(
                text = locale.usageStatsButton,
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.weight(1f),
            )
            FilledTonalButton(
                onClick = onClick,
                shape = CircleShape,
            ) {
                Text(
                    text = locale.usageStatsSettingsAction,
                    style = MaterialTheme.typography.labelMediumEmphasized,
                )
            }
        }
    }
}

@Composable
internal fun ResetDefaultsCard(
    locale: MobileLocaleText,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var showConfirm by remember { mutableStateOf(false) }
    val context = androidx.compose.ui.platform.LocalContext.current
    val doneMsg = when {
        locale.resetDefaultsButton.contains("Khôi") -> "Đã khôi phục mặc định"
        locale.resetDefaultsButton.contains("복원") -> "기본값으로 복원됨"
        else -> "Defaults restored"
    }

    Card(
        modifier = modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.large,
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
            Icon(
                Icons.Rounded.RestartAlt,
                contentDescription = null,
                modifier = Modifier.size(22.dp),
                tint = MaterialTheme.colorScheme.error,
            )
            Text(
                text = locale.resetDefaultsButton,
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.weight(1f),
            )
            FilledTonalButton(
                onClick = {
                    showConfirm = true
                },
                shape = CircleShape,
            ) {
                Text(
                    text = locale.resetDefaultsAction,
                    style = MaterialTheme.typography.labelMediumEmphasized,
                )
            }
        }
    }

    if (showConfirm) {
        AlertDialog(
            onDismissRequest = { showConfirm = false },
            title = { Text(locale.resetDefaultsConfirmTitle) },
            text = { Text(locale.resetDefaultsConfirmMessage) },
            confirmButton = {
                TextButton(
                    onClick = {
                        showConfirm = false
                        onClick()
                        android.widget.Toast.makeText(context, doneMsg, android.widget.Toast.LENGTH_SHORT).show()
                    },
                ) {
                    Text(locale.resetDefaultsAction)
                }
            },
            dismissButton = {
                TextButton(onClick = { showConfirm = false }) {
                    Text(locale.closeLabel)
                }
            },
        )
    }
}
