package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.ui.res.painterResource
import dev.screengoated.toolbox.mobile.R
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import dev.screengoated.toolbox.mobile.updater.AppUpdateStatus
import dev.screengoated.toolbox.mobile.updater.AppUpdateUiState
import dev.screengoated.toolbox.mobile.updater.openAppUpdate

@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
internal fun AppUpdateSection(
    state: AppUpdateUiState,
    locale: MobileLocaleText,
    onCheckForUpdates: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val accent = when (state.status) {
        AppUpdateStatus.UPDATE_AVAILABLE -> MaterialTheme.colorScheme.primary
        AppUpdateStatus.ERROR -> MaterialTheme.colorScheme.error
        AppUpdateStatus.UP_TO_DATE -> MaterialTheme.colorScheme.tertiary
        else -> MaterialTheme.colorScheme.secondary
    }

    ExpressiveSettingsCard(
        accent = accent,
        modifier = modifier.fillMaxWidth(),
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(ShellSpacing.itemGap)) {
            ExpressiveSettingsHeader(
                title = locale.softwareUpdateHeader,
                icon = R.drawable.ms_upgrade,
                accent = accent,
            )
            when (state.status) {
                AppUpdateStatus.IDLE -> {
                    UpdateSectionStatusRow(
                        label = "${locale.currentVersionLabel} v${state.currentVersion}",
                        buttonText = locale.checkForUpdatesButton,
                        onClick = onCheckForUpdates,
                    )
                }

                AppUpdateStatus.CHECKING -> {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(18.dp),
                            strokeWidth = 2.dp,
                        )
                        Text(
                            text = locale.checkingGithub,
                            style = MaterialTheme.typography.bodyMedium,
                        )
                    }
                }

                AppUpdateStatus.UP_TO_DATE -> {
                    UpdateSectionStatusRow(
                        label = "${locale.upToDateLabel} (v${state.currentVersion})",
                        buttonText = locale.checkAgainButton,
                        onClick = onCheckForUpdates,
                        color = MaterialTheme.colorScheme.tertiary,
                    )
                }

                AppUpdateStatus.UPDATE_AVAILABLE -> {
                    Text(
                        text = "${locale.newVersionAvailableLabel} ${state.latestVersion.orEmpty()}",
                        style = MaterialTheme.typography.titleSmall,
                        color = accent,
                        fontWeight = FontWeight.SemiBold,
                    )
                    if (state.releaseNotes.isNotBlank()) {
                        ExpressiveSettingsInsetCard(
                            accent = accent,
                            verticalPadding = 10.dp,
                        ) {
                            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                                Row(
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                                ) {
                                    Icon(
                                        painter = painterResource(R.drawable.ms_info),
                                        contentDescription = null,
                                        tint = accent,
                                        modifier = Modifier.size(16.dp),
                                    )
                                    Text(
                                        text = locale.releaseNotesLabel,
                                        style = MaterialTheme.typography.labelLarge,
                                        color = accent,
                                    )
                                }
                                Text(
                                    text = state.releaseNotes.trim(),
                                    style = MaterialTheme.typography.bodySmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                        }
                    }
                    UpdateSectionPrimaryButton(
                        text = locale.downloadUpdateButton,
                        onClick = { openAppUpdate(context, state) },
                    )
                }

                AppUpdateStatus.ERROR -> {
                    Text(
                        text = "${locale.updateFailedLabel} ${state.errorMessage.orEmpty()}",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.error,
                    )
                    UpdateSectionPrimaryButton(
                        text = locale.retryButton,
                        onClick = onCheckForUpdates,
                    )
                }
            }
        }
    }
}

@Composable
private fun UpdateSectionStatusRow(
    label: String,
    buttonText: String,
    onClick: () -> Unit,
    color: Color = MaterialTheme.colorScheme.onSurface,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ShellSpacing.itemGap),
    ) {
        Text(
            text = label,
            style = MaterialTheme.typography.bodyMedium,
            color = color,
            modifier = Modifier.weight(1f),
        )
        ExpressiveSettingsButton(
            text = buttonText,
            onClick = onClick,
            accent = MaterialTheme.colorScheme.primary,
        )
    }
}

@Composable
private fun UpdateSectionPrimaryButton(
    text: String,
    onClick: () -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.End,
    ) {
        SettingsActionButton(
            text = text,
            icon = R.drawable.ms_download,
            onClick = onClick,
            morphStyle = SettingsActionMorphStyle.STATS,
        )
    }
}
