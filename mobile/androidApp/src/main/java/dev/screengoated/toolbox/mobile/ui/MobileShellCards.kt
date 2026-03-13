@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.PowerSettingsNew
import androidx.compose.material3.Card
import androidx.compose.material3.ElevatedCard
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionState
import dev.screengoated.toolbox.mobile.shared.live.SessionPhase
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

@Composable
internal fun CredentialsCard(
    apiKey: String,
    cerebrasApiKey: String,
    locale: MobileLocaleText,
    onApiKeyChanged: (String) -> Unit,
    onCerebrasApiKeyChanged: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    Card(
        modifier = modifier,
        shape = MaterialTheme.shapes.extraLarge,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(18.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Text(
                text = locale.shellCredentialsTitle,
                style = MaterialTheme.typography.titleLargeEmphasized,
            )
            Text(
                text = locale.shellCredentialsDescription,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            HorizontalDivider()
            OutlinedTextField(
                modifier = Modifier.fillMaxWidth(),
                value = apiKey,
                onValueChange = onApiKeyChanged,
                label = { Text(locale.geminiKeyLabel) },
                singleLine = true,
                visualTransformation = PasswordVisualTransformation(),
                shape = MaterialTheme.shapes.large,
            )
            OutlinedTextField(
                modifier = Modifier.fillMaxWidth(),
                value = cerebrasApiKey,
                onValueChange = onCerebrasApiKeyChanged,
                label = { Text(locale.cerebrasKeyLabel) },
                singleLine = true,
                visualTransformation = PasswordVisualTransformation(),
                shape = MaterialTheme.shapes.large,
            )
        }
    }
}

@Composable
internal fun VoiceSettingsCard(
    globalTtsSettings: MobileGlobalTtsSettings,
    locale: MobileLocaleText,
    onVoiceSettingsClick: () -> Unit,
) {
    ElevatedCard(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.extraLarge,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(18.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(
                text = locale.shellVoiceTitle,
                style = MaterialTheme.typography.titleLargeEmphasized,
            )
            Text(
                text = locale.shellVoiceDescription,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            StatusChip(
                label = methodLabel(locale, globalTtsSettings.method),
                accent = MaterialTheme.colorScheme.secondary,
            )
            FilledTonalButton(
                modifier = Modifier.fillMaxWidth(),
                shape = MaterialTheme.shapes.largeIncreased,
                onClick = onVoiceSettingsClick,
            ) {
                Text(
                    text = locale.voiceSettingsButton,
                    style = MaterialTheme.typography.labelLargeEmphasized,
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
    ElevatedCard(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.extraLarge,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(18.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(
                text = locale.shellLiveTitle,
                style = MaterialTheme.typography.titleLargeEmphasized,
            )
            Text(
                text = locale.shellLiveDescription,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                StatusChip(
                    label = statusLabelForPhase(locale, state.phase),
                    accent = when (state.phase) {
                        SessionPhase.STARTING -> MaterialTheme.colorScheme.tertiary
                        SessionPhase.TRANSLATING -> MaterialTheme.colorScheme.secondary
                        SessionPhase.LISTENING -> MaterialTheme.colorScheme.primary
                        else -> MaterialTheme.colorScheme.outline
                    },
                )
                Spacer(modifier = Modifier.weight(1f))
                FilledTonalButton(
                    onClick = onSessionToggle,
                    enabled = canToggle,
                    shape = MaterialTheme.shapes.largeIncreased,
                ) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Icon(Icons.Rounded.PowerSettingsNew, contentDescription = null)
                        Text(if (state.phase == SessionPhase.STOPPED) locale.turnOn else locale.turnOff)
                    }
                }
            }
        }
    }
}
