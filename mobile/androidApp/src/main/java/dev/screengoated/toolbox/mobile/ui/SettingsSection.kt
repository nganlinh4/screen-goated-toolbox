@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.BarChart
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.preset.PresetRuntimeSettings
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

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
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier.verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(ShellSpacing.cardGap),
    ) {
        if (wideLayout) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(ShellSpacing.cardGap),
            ) {
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
                    modifier = Modifier.weight(1.15f),
                )
                VoiceSettingsCard(
                    globalTtsSettings = globalTtsSettings,
                    locale = locale,
                    onVoiceSettingsClick = onVoiceSettingsClick,
                    modifier = Modifier.weight(0.85f),
                )
            }
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(ShellSpacing.cardGap),
            ) {
                SettingsActionButton(
                    text = locale.presetRuntimeButton,
                    icon = Icons.Rounded.Settings,
                    onClick = onPresetRuntimeSettingsClick,
                    morphStyle = SettingsActionMorphStyle.PRIORITY,
                    modifier = Modifier.weight(1f),
                )
                SettingsActionButton(
                    text = locale.usageStatsButton,
                    icon = Icons.Rounded.BarChart,
                    onClick = onUsageStatsClick,
                    morphStyle = SettingsActionMorphStyle.STATS,
                    modifier = Modifier.weight(1f),
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
                    icon = Icons.Rounded.Settings,
                    onClick = onPresetRuntimeSettingsClick,
                    morphStyle = SettingsActionMorphStyle.PRIORITY,
                    modifier = Modifier.weight(1f),
                )
                SettingsActionButton(
                    text = locale.usageStatsButton,
                    icon = Icons.Rounded.BarChart,
                    onClick = onUsageStatsClick,
                    morphStyle = SettingsActionMorphStyle.STATS,
                    modifier = Modifier.weight(1f),
                )
            }
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
        }
        OverlayOpacityCard(
            opacityPercent = overlayOpacityPercent,
            locale = locale,
            onOpacityChanged = onOverlayOpacityChanged,
        )
        DownloadedToolsSection(locale = locale)
    }
}
