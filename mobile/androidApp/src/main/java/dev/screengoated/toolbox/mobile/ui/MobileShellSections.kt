@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.HelpOutline
import androidx.compose.material.icons.rounded.Download
import androidx.compose.material.icons.rounded.GraphicEq
import androidx.compose.material.icons.rounded.History
import androidx.compose.material.icons.rounded.Tune
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ElevatedCard
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.WideNavigationRail
import androidx.compose.material3.WideNavigationRailItem
import androidx.compose.material3.WideNavigationRailValue
import androidx.compose.material3.rememberWideNavigationRailState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionState
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

@Composable
internal fun ShellRail(
    selectedSection: MobileShellSection,
    onSectionSelected: (MobileShellSection) -> Unit,
    locale: MobileLocaleText,
    modifier: Modifier = Modifier,
) {
    val railState = rememberWideNavigationRailState(WideNavigationRailValue.Expanded)
    Card(
        modifier = modifier.width(220.dp),
        shape = MaterialTheme.shapes.extraLarge,
    ) {
        WideNavigationRail(
            state = railState,
            modifier = Modifier.fillMaxHeight(),
            header = {
                Column(
                    modifier = Modifier.padding(horizontal = 18.dp, vertical = 16.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Text(
                        text = locale.shellSectionTitle,
                        style = MaterialTheme.typography.labelLargeEmphasized,
                    )
                    Text(
                        text = locale.shellCurrentSectionLabel,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            },
        ) {
            ShellRailItem(
                selected = selectedSection == MobileShellSection.GLOBAL,
                onClick = { onSectionSelected(MobileShellSection.GLOBAL) },
                icon = Icons.Rounded.Tune,
                label = locale.shellGlobalLabel,
                description = locale.shellGlobalDescription,
            )
            ShellRailItem(
                selected = selectedSection == MobileShellSection.HISTORY,
                onClick = { onSectionSelected(MobileShellSection.HISTORY) },
                icon = Icons.Rounded.History,
                label = locale.shellHistoryLabel,
                description = locale.shellHistoryDescription,
            )
            ShellRailItem(
                selected = selectedSection == MobileShellSection.PRESETS,
                onClick = { onSectionSelected(MobileShellSection.PRESETS) },
                icon = Icons.Rounded.GraphicEq,
                label = locale.shellPresetsLabel,
                description = locale.shellPresetsDescription,
            )
        }
    }
}

@Composable
private fun ShellRailItem(
    selected: Boolean,
    onClick: () -> Unit,
    icon: ImageVector,
    label: String,
    description: String,
) {
    WideNavigationRailItem(
        selected = selected,
        onClick = onClick,
        icon = { Icon(icon, contentDescription = null) },
        label = {
            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(label)
                Text(
                    text = description,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 2,
                )
            }
        },
        railExpanded = true,
    )
}

@Composable
internal fun SectionDeck(
    selectedSection: MobileShellSection,
    onSectionSelected: (MobileShellSection) -> Unit,
    locale: MobileLocaleText,
) {
    Card(shape = MaterialTheme.shapes.extraLarge) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(18.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Text(
                text = locale.shellSectionTitle,
                style = MaterialTheme.typography.titleLargeEmphasized,
            )
            SectionTile(
                label = locale.shellGlobalLabel,
                description = locale.shellGlobalDescription,
                icon = Icons.Rounded.Tune,
                selected = selectedSection == MobileShellSection.GLOBAL,
                onClick = { onSectionSelected(MobileShellSection.GLOBAL) },
                brush = Brush.linearGradient(
                    listOf(
                        MaterialTheme.colorScheme.primary,
                        MaterialTheme.colorScheme.tertiary,
                    ),
                ),
            )
            SectionTile(
                label = locale.shellHistoryLabel,
                description = locale.shellHistoryDescription,
                icon = Icons.Rounded.History,
                selected = selectedSection == MobileShellSection.HISTORY,
                onClick = { onSectionSelected(MobileShellSection.HISTORY) },
                brush = Brush.linearGradient(
                    listOf(
                        MaterialTheme.colorScheme.secondary,
                        MaterialTheme.colorScheme.primary,
                    ),
                ),
            )
            SectionTile(
                label = locale.shellPresetsLabel,
                description = locale.shellPresetsDescription,
                icon = Icons.Rounded.GraphicEq,
                selected = selectedSection == MobileShellSection.PRESETS,
                onClick = { onSectionSelected(MobileShellSection.PRESETS) },
                brush = Brush.linearGradient(
                    listOf(
                        MaterialTheme.colorScheme.tertiary,
                        MaterialTheme.colorScheme.secondary,
                    ),
                ),
            )
        }
    }
}

@Composable
internal fun QuickActionsRow(locale: MobileLocaleText) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(
            text = locale.shellUtilitiesTitle,
            style = MaterialTheme.typography.titleMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            UtilityTile(
                label = locale.shellDownloadsLabel,
                description = locale.shellDownloadsDescription,
                icon = Icons.Rounded.Download,
                brush = Brush.linearGradient(
                    listOf(
                        MaterialTheme.colorScheme.secondary,
                        MaterialTheme.colorScheme.primary,
                    ),
                ),
                modifier = Modifier.weight(1f),
            )
            UtilityTile(
                label = locale.shellHelpLabel,
                description = locale.shellHelpDescription,
                icon = Icons.AutoMirrored.Rounded.HelpOutline,
                brush = Brush.linearGradient(
                    listOf(
                        MaterialTheme.colorScheme.tertiary,
                        MaterialTheme.colorScheme.primary,
                    ),
                ),
                modifier = Modifier.weight(1f),
            )
        }
    }
}

@Composable
private fun UtilityTile(
    label: String,
    description: String,
    icon: ImageVector,
    brush: Brush,
    modifier: Modifier = Modifier,
) {
    ElevatedCard(
        modifier = modifier,
        shape = MaterialTheme.shapes.extraLarge,
        colors = CardDefaults.elevatedCardColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainerLowest,
        ),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(18.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Surface(
                modifier = Modifier.size(54.dp),
                shape = androidx.compose.foundation.shape.CircleShape,
                color = MaterialTheme.colorScheme.surfaceContainerHighest,
            ) {
                GradientMaskedIcon(icon, brush, modifier = Modifier.padding(14.dp))
            }
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(
                    text = label,
                    style = MaterialTheme.typography.titleMedium,
                )
                Text(
                    text = description,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

@Composable
internal fun SectionDetail(
    selectedSection: MobileShellSection,
    state: LiveSessionState,
    apiKey: String,
    cerebrasApiKey: String,
    globalTtsSettings: MobileGlobalTtsSettings,
    locale: MobileLocaleText,
    wideLayout: Boolean,
    onApiKeyChanged: (String) -> Unit,
    onCerebrasApiKeyChanged: (String) -> Unit,
    onVoiceSettingsClick: () -> Unit,
    onSessionToggle: () -> Unit,
    canToggle: Boolean,
) {
    when (selectedSection) {
        MobileShellSection.GLOBAL -> GlobalSection(
            state = state,
            apiKey = apiKey,
            cerebrasApiKey = cerebrasApiKey,
            globalTtsSettings = globalTtsSettings,
            locale = locale,
            wideLayout = wideLayout,
            onApiKeyChanged = onApiKeyChanged,
            onCerebrasApiKeyChanged = onCerebrasApiKeyChanged,
            onVoiceSettingsClick = onVoiceSettingsClick,
            onSessionToggle = onSessionToggle,
            canToggle = canToggle,
        )

        MobileShellSection.HISTORY -> PlaceholderSection(
            label = locale.shellHistoryLabel,
            description = locale.shellHistoryDescription,
            locale = locale,
        )

        MobileShellSection.PRESETS -> PlaceholderSection(
            label = locale.shellPresetsLabel,
            description = locale.shellPresetsDescription,
            locale = locale,
        )
    }
}

@Composable
internal fun GlobalSection(
    state: LiveSessionState,
    apiKey: String,
    cerebrasApiKey: String,
    globalTtsSettings: MobileGlobalTtsSettings,
    locale: MobileLocaleText,
    wideLayout: Boolean,
    onApiKeyChanged: (String) -> Unit,
    onCerebrasApiKeyChanged: (String) -> Unit,
    onVoiceSettingsClick: () -> Unit,
    onSessionToggle: () -> Unit,
    canToggle: Boolean,
) {
    if (wideLayout) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            CredentialsCard(
                apiKey = apiKey,
                cerebrasApiKey = cerebrasApiKey,
                locale = locale,
                onApiKeyChanged = onApiKeyChanged,
                onCerebrasApiKeyChanged = onCerebrasApiKeyChanged,
                modifier = Modifier.weight(1.15f),
            )
            Column(
                modifier = Modifier.weight(0.85f),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                VoiceSettingsCard(
                    globalTtsSettings = globalTtsSettings,
                    locale = locale,
                    onVoiceSettingsClick = onVoiceSettingsClick,
                )
                LiveControlCard(
                    state = state,
                    locale = locale,
                    onSessionToggle = onSessionToggle,
                    canToggle = canToggle,
                )
            }
        }
    } else {
        Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
            CredentialsCard(
                apiKey = apiKey,
                cerebrasApiKey = cerebrasApiKey,
                locale = locale,
                onApiKeyChanged = onApiKeyChanged,
                onCerebrasApiKeyChanged = onCerebrasApiKeyChanged,
                modifier = Modifier.fillMaxWidth(),
            )
            VoiceSettingsCard(
                globalTtsSettings = globalTtsSettings,
                locale = locale,
                onVoiceSettingsClick = onVoiceSettingsClick,
            )
            LiveControlCard(
                state = state,
                locale = locale,
                onSessionToggle = onSessionToggle,
                canToggle = canToggle,
            )
        }
    }
}

@Composable
internal fun PlaceholderSection(
    label: String,
    description: String,
    locale: MobileLocaleText,
) {
    Card(shape = MaterialTheme.shapes.extraLarge) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(18.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            StatusChip(
                label = locale.shellPlaceholderBadge,
                accent = MaterialTheme.colorScheme.outline,
            )
            Text(
                text = label,
                style = MaterialTheme.typography.titleLargeEmphasized,
            )
            Text(
                text = description,
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            androidx.compose.material3.HorizontalDivider()
            Text(
                text = locale.shellPlaceholderMessage,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}
