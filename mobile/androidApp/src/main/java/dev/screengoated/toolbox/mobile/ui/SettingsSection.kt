@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import dev.screengoated.toolbox.mobile.R
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.preset.PresetRuntimeSettings
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import dev.screengoated.toolbox.mobile.updater.AppUpdateUiState

@Composable
internal fun GlobalSection(
    apiKey: String,
    cerebrasApiKey: String,
    groqApiKey: String,
    openRouterApiKey: String,
    ollamaUrl: String,
    globalTtsSettings: MobileGlobalTtsSettings,
    presetRuntimeSettings: PresetRuntimeSettings,
    overlayOpacityPercent: Int,
    appUpdateState: AppUpdateUiState,
    locale: MobileLocaleText,
    wideLayout: Boolean,
    onApiKeyChanged: (String) -> Unit,
    onCerebrasApiKeyChanged: (String) -> Unit,
    onGroqApiKeyChanged: (String) -> Unit,
    onOpenRouterApiKeyChanged: (String) -> Unit,
    onOllamaUrlChanged: (String) -> Unit,
    onPresetRuntimeSettingsClick: () -> Unit,
    onUsageStatsClick: () -> Unit,
    onResetDefaults: () -> Unit,
    onVoiceSettingsClick: () -> Unit,
    onOverlayOpacityChanged: (Int) -> Unit,
    onCheckForAppUpdates: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val isLandscape =
        LocalConfiguration.current.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE
    val compactLandscape = isLandscape && !wideLayout

    Column(
        modifier = modifier.verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(ShellSpacing.cardGap),
    ) {
        if (wideLayout || compactLandscape) {
            CredentialsCard(
                apiKey = apiKey,
                cerebrasApiKey = cerebrasApiKey,
                groqApiKey = groqApiKey,
                openRouterApiKey = openRouterApiKey,
                ollamaUrl = ollamaUrl,
                locale = locale,
                onApiKeyChanged = onApiKeyChanged,
                onCerebrasApiKeyChanged = onCerebrasApiKeyChanged,
                onGroqApiKeyChanged = onGroqApiKeyChanged,
                onOpenRouterApiKeyChanged = onOpenRouterApiKeyChanged,
                onOllamaUrlChanged = onOllamaUrlChanged,
                modifier = Modifier.fillMaxWidth(),
            )
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(ShellSpacing.cardGap),
            ) {
                VoiceSettingsCard(
                    globalTtsSettings = globalTtsSettings,
                    locale = locale,
                    onVoiceSettingsClick = onVoiceSettingsClick,
                    modifier = Modifier.weight(2f),
                )
                SettingsActionButton(
                    text = locale.presetRuntimeButton,
                    icon = R.drawable.ms_settings,
                    onClick = onPresetRuntimeSettingsClick,
                    morphStyle = SettingsActionMorphStyle.PRIORITY,
                    modifier = Modifier.weight(1f),
                )
                SettingsActionButton(
                    text = locale.usageStatsButton,
                    icon = R.drawable.ms_bar_chart,
                    onClick = onUsageStatsClick,
                    morphStyle = SettingsActionMorphStyle.STATS,
                    modifier = Modifier.weight(1f),
                )
            }
            UsageTipsCard(locale = locale)
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(ShellSpacing.cardGap),
            ) {
                HelpAssistantActionButton(
                    locale = locale,
                    modifier = Modifier.weight(1f),
                )
                ResetDefaultsActionButton(
                    locale = locale,
                    onClick = onResetDefaults,
                    modifier = Modifier.weight(1f),
                )
                OverlayOpacityCard(
                    opacityPercent = overlayOpacityPercent,
                    locale = locale,
                    onOpacityChanged = onOverlayOpacityChanged,
                    compact = true,
                    modifier = Modifier.weight(2f),
                )
            }
        } else {
            CredentialsCard(
                apiKey = apiKey,
                cerebrasApiKey = cerebrasApiKey,
                groqApiKey = groqApiKey,
                openRouterApiKey = openRouterApiKey,
                ollamaUrl = ollamaUrl,
                locale = locale,
                onApiKeyChanged = onApiKeyChanged,
                onCerebrasApiKeyChanged = onCerebrasApiKeyChanged,
                onGroqApiKeyChanged = onGroqApiKeyChanged,
                onOpenRouterApiKeyChanged = onOpenRouterApiKeyChanged,
                onOllamaUrlChanged = onOllamaUrlChanged,
                modifier = Modifier.fillMaxWidth(),
            )
            VoiceSettingsCard(
                globalTtsSettings = globalTtsSettings,
                locale = locale,
                onVoiceSettingsClick = onVoiceSettingsClick,
            )
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(ShellSpacing.cardGap),
            ) {
                SettingsActionButton(
                    text = locale.presetRuntimeButton,
                    icon = R.drawable.ms_settings,
                    onClick = onPresetRuntimeSettingsClick,
                    morphStyle = SettingsActionMorphStyle.PRIORITY,
                    modifier = Modifier.weight(1f),
                )
                SettingsActionButton(
                    text = locale.usageStatsButton,
                    icon = R.drawable.ms_bar_chart,
                    onClick = onUsageStatsClick,
                    morphStyle = SettingsActionMorphStyle.STATS,
                    modifier = Modifier.weight(1f),
                )
            }
        }
        if (!(wideLayout || compactLandscape)) {
            UsageTipsCard(locale = locale)
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(ShellSpacing.cardGap),
            ) {
                HelpAssistantActionButton(
                    locale = locale,
                    modifier = Modifier.weight(1f),
                )
                ResetDefaultsActionButton(
                    locale = locale,
                    onClick = onResetDefaults,
                    modifier = Modifier.weight(1f),
                )
            }
            OverlayOpacityCard(
                opacityPercent = overlayOpacityPercent,
                locale = locale,
                onOpacityChanged = onOverlayOpacityChanged,
            )
        }
        AppUpdateSection(
            state = appUpdateState,
            locale = locale,
            onCheckForUpdates = onCheckForAppUpdates,
        )
        DownloadedToolsSection(locale = locale)
    }
}
