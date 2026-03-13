@file:OptIn(ExperimentalMaterial3Api::class, ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.DarkMode
import androidx.compose.material.icons.rounded.LightMode
import androidx.compose.material.icons.rounded.SettingsBrightness
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LargeFlexibleTopAppBar
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import dev.screengoated.toolbox.mobile.model.MobileEdgeTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileThemeMode
import dev.screengoated.toolbox.mobile.model.MobileTtsLanguageCondition
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset
import dev.screengoated.toolbox.mobile.model.MobileUiPreferences
import dev.screengoated.toolbox.mobile.service.tts.EdgeVoiceCatalogState
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionState
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

@Composable
fun SgtMobileApp(
    state: LiveSessionState,
    apiKey: String,
    cerebrasApiKey: String,
    globalTtsSettings: MobileGlobalTtsSettings,
    uiPreferences: MobileUiPreferences,
    locale: MobileLocaleText,
    edgeVoiceCatalogState: EdgeVoiceCatalogState,
    onApiKeyChanged: (String) -> Unit,
    onCerebrasApiKeyChanged: (String) -> Unit,
    onUiLanguageSelected: (String) -> Unit,
    onThemeCycleRequested: () -> Unit,
    onGlobalTtsMethodChanged: (MobileTtsMethod) -> Unit,
    onGlobalTtsSpeedPresetChanged: (MobileTtsSpeedPreset) -> Unit,
    onGlobalTtsVoiceChanged: (String) -> Unit,
    onGlobalTtsConditionsChanged: (List<MobileTtsLanguageCondition>) -> Unit,
    onGlobalEdgeTtsSettingsChanged: (MobileEdgeTtsSettings) -> Unit,
    onVoiceSettingsShown: () -> Unit,
    onRetryEdgeVoiceCatalog: () -> Unit,
    onPreviewGeminiVoice: (String) -> Unit,
    onPreviewEdgeVoice: (String, String) -> Unit,
    onSessionToggle: () -> Unit,
) {
    var showTtsSettings by rememberSaveable { mutableStateOf(false) }
    var showLanguageMenu by rememberSaveable { mutableStateOf(false) }

    if (showTtsSettings) {
        GlobalTtsSettingsDialog(
            settings = globalTtsSettings,
            locale = locale,
            edgeVoiceCatalogState = edgeVoiceCatalogState,
            onDismiss = { showTtsSettings = false },
            onMethodChanged = onGlobalTtsMethodChanged,
            onSpeedPresetChanged = onGlobalTtsSpeedPresetChanged,
            onVoiceChanged = onGlobalTtsVoiceChanged,
            onConditionsChanged = onGlobalTtsConditionsChanged,
            onEdgeSettingsChanged = onGlobalEdgeTtsSettingsChanged,
            onRetryEdgeVoiceCatalog = onRetryEdgeVoiceCatalog,
            onPreviewGeminiVoice = onPreviewGeminiVoice,
            onPreviewEdgeVoice = onPreviewEdgeVoice,
        )
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(
                Brush.linearGradient(
                    colors = listOf(
                        MaterialTheme.colorScheme.surface,
                        MaterialTheme.colorScheme.surfaceContainer.copy(alpha = 0.92f),
                        MaterialTheme.colorScheme.surfaceBright.copy(alpha = 0.98f),
                    ),
                ),
            ),
    ) {
        Scaffold(
            containerColor = Color.Transparent,
            topBar = {
                LargeFlexibleTopAppBar(
                    title = {
                        Text(
                            text = locale.appTitle,
                            style = MaterialTheme.typography.headlineSmallEmphasized,
                        )
                    },
                    subtitle = {
                        Text(
                            text = locale.appSubtitle,
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    },
                    navigationIcon = {
                        Box {
                            FilledTonalButton(onClick = { showLanguageMenu = true }) {
                                Text(
                                    text = locale.languageOptions.firstOrNull {
                                        it.code == uiPreferences.uiLanguage
                                    }?.label ?: locale.languageOptions.first().label,
                                    style = MaterialTheme.typography.labelLargeEmphasized,
                                )
                            }
                            DropdownMenu(
                                expanded = showLanguageMenu,
                                onDismissRequest = { showLanguageMenu = false },
                            ) {
                                locale.languageOptions.forEach { option ->
                                    DropdownMenuItem(
                                        text = { Text(option.label) },
                                        onClick = {
                                            onUiLanguageSelected(option.code)
                                            showLanguageMenu = false
                                        },
                                    )
                                }
                            }
                        }
                    },
                    actions = {
                        IconButton(onClick = onThemeCycleRequested) {
                            Icon(
                                imageVector = themeIcon(uiPreferences.themeMode),
                                contentDescription = "${locale.themeCycleLabel}: ${locale.themeModeLabels[uiPreferences.themeMode]}",
                            )
                        }
                    },
                )
            },
        ) { innerPadding ->
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .background(Color.Transparent)
                    .padding(innerPadding),
            ) {
                MobileShellSurface(
                    state = state,
                    apiKey = apiKey,
                    cerebrasApiKey = cerebrasApiKey,
                    globalTtsSettings = globalTtsSettings,
                    locale = locale,
                    onApiKeyChanged = onApiKeyChanged,
                    onCerebrasApiKeyChanged = onCerebrasApiKeyChanged,
                    onVoiceSettingsClick = {
                        onVoiceSettingsShown()
                        showTtsSettings = true
                    },
                    onSessionToggle = onSessionToggle,
                )
            }
        }
    }
}

private fun themeIcon(themeMode: MobileThemeMode) = when (themeMode) {
    MobileThemeMode.SYSTEM -> Icons.Rounded.SettingsBrightness
    MobileThemeMode.DARK -> Icons.Rounded.DarkMode
    MobileThemeMode.LIGHT -> Icons.Rounded.LightMode
}
