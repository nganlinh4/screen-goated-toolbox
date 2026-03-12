package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.model.MobileEdgeTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileTtsLanguageCondition
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionState
import dev.screengoated.toolbox.mobile.shared.live.SessionPhase

@Composable
fun SgtMobileApp(
    state: LiveSessionState,
    apiKey: String,
    cerebrasApiKey: String,
    globalTtsSettings: MobileGlobalTtsSettings,
    onApiKeyChanged: (String) -> Unit,
    onCerebrasApiKeyChanged: (String) -> Unit,
    onGlobalTtsMethodChanged: (MobileTtsMethod) -> Unit,
    onGlobalTtsSpeedPresetChanged: (MobileTtsSpeedPreset) -> Unit,
    onGlobalTtsVoiceChanged: (String) -> Unit,
    onGlobalTtsConditionsChanged: (List<MobileTtsLanguageCondition>) -> Unit,
    onGlobalEdgeTtsSettingsChanged: (MobileEdgeTtsSettings) -> Unit,
    onSessionToggle: () -> Unit,
) {
    val isActive = state.phase in setOf(
        SessionPhase.STARTING,
        SessionPhase.LISTENING,
        SessionPhase.TRANSLATING,
    )
    val canToggle = apiKey.isNotBlank() || isActive
    var showTtsSettings by rememberSaveable { mutableStateOf(false) }

    if (showTtsSettings) {
        GlobalTtsSettingsDialog(
            settings = globalTtsSettings,
            onDismiss = { showTtsSettings = false },
            onMethodChanged = onGlobalTtsMethodChanged,
            onSpeedPresetChanged = onGlobalTtsSpeedPresetChanged,
            onVoiceChanged = onGlobalTtsVoiceChanged,
            onConditionsChanged = onGlobalTtsConditionsChanged,
            onEdgeSettingsChanged = onGlobalEdgeTtsSettingsChanged,
        )
    }

    Surface(
        modifier = Modifier.fillMaxSize(),
        color = MaterialTheme.colorScheme.surface,
    ) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 24.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
        ) {
            Column(
                modifier = Modifier.widthIn(max = 420.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                OutlinedTextField(
                    modifier = Modifier.fillMaxWidth(),
                    value = apiKey,
                    onValueChange = onApiKeyChanged,
                    label = { Text("Gemini key") },
                    singleLine = true,
                    visualTransformation = PasswordVisualTransformation(),
                    shape = MaterialTheme.shapes.large,
                )
                OutlinedTextField(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(top = 12.dp),
                    value = cerebrasApiKey,
                    onValueChange = onCerebrasApiKeyChanged,
                    label = { Text("Cerebras key") },
                    singleLine = true,
                    visualTransformation = PasswordVisualTransformation(),
                    shape = MaterialTheme.shapes.large,
                )
                OutlinedButton(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(top = 12.dp),
                    onClick = { showTtsSettings = true },
                    shape = MaterialTheme.shapes.large,
                ) {
                    Text("Voice Settings")
                }
                Button(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(top = 16.dp),
                    onClick = onSessionToggle,
                    enabled = canToggle,
                    shape = MaterialTheme.shapes.extraLarge,
                ) {
                    Text(if (isActive) "Turn off" else "Turn on")
                }
            }
        }
    }
}
