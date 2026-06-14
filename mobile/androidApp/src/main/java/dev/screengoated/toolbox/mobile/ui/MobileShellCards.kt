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
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.annotation.DrawableRes
import androidx.compose.ui.res.painterResource
import dev.screengoated.toolbox.mobile.R
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialShapes
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.foundation.clickable
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

private data class ProviderDef(
    val label: String,
    @DrawableRes val icon: Int,
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
                icon = R.drawable.ms_electric_bolt,
                keyLabel = locale.groqKeyLabel,
                getKeyUrl = "https://console.groq.com/keys",
                getKeyLabel = locale.groqGetKeyLink,
            )
            CredentialsProviderId.CEREBRAS -> ProviderDef(
                label = provider.label,
                icon = R.drawable.ms_local_fire_department,
                keyLabel = locale.cerebrasKeyLabel,
                getKeyUrl = "https://cloud.cerebras.ai/",
                getKeyLabel = locale.cerebrasGetKeyLink,
            )
            CredentialsProviderId.GEMINI -> ProviderDef(
                label = provider.label,
                icon = R.drawable.ms_auto_awesome,
                keyLabel = locale.geminiKeyLabel,
                getKeyUrl = "https://aistudio.google.com/app/apikey",
                getKeyLabel = locale.geminiGetKeyLink,
            )
            CredentialsProviderId.OPEN_ROUTER -> ProviderDef(
                label = provider.label,
                icon = R.drawable.ms_public,
                keyLabel = locale.openRouterKeyLabel,
                getKeyUrl = "https://openrouter.ai/settings/keys",
                getKeyLabel = locale.openRouterGetKeyLink,
            )
            CredentialsProviderId.OLLAMA -> ProviderDef(
                label = provider.label,
                icon = R.drawable.ms_terminal,
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
                icon = R.drawable.ms_key,
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
                            Row(
                                modifier = Modifier.widthIn(min = 132.dp),
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
                                        painter = painterResource(provider.icon),
                                        contentDescription = null,
                                        tint = accent,
                                        modifier = Modifier.size(14.dp),
                                    )
                                }
                                Text(
                                    text = provider.label,
                                    style = MaterialTheme.typography.labelMediumEmphasized,
                                    color = accent,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                            }
                            Box(
                                modifier = Modifier
                                    .weight(1f)
                                    .fillMaxWidth(),
                                contentAlignment = Alignment.Center,
                            ) {
                                if (expanded && provider.getKeyUrl != null && provider.getKeyLabel != null) {
                                    Text(
                                        text = provider.getKeyLabel,
                                        style = MaterialTheme.typography.labelSmall,
                                        color = accent,
                                        textAlign = TextAlign.Center,
                                        maxLines = 1,
                                        modifier = Modifier
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
