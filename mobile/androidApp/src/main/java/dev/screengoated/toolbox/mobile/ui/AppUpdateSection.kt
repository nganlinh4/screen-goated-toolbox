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
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import dev.screengoated.toolbox.mobile.BuildConfig
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import dev.screengoated.toolbox.mobile.updater.AppUpdateStatus
import dev.screengoated.toolbox.mobile.updater.AppUpdateUiState
import dev.screengoated.toolbox.mobile.updater.PlayInAppUpdateManager
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
    // The `play` flavor drives Google Play In-App Updates; the `full` (sideload)
    // flavor keeps the GitHub-release flow. See `.claude/parity/app-update.md`.
    val isPlayFlavor = BuildConfig.FLAVOR == "play"
    val playController = if (isPlayFlavor) {
        remember(context) {
            (context.applicationContext as? SgtMobileApplication)
                ?.appContainer
                ?.appUpdateController as? PlayInAppUpdateManager
        }
    } else {
        null
    }
    // Flexible-update acceptance is reported here; download progress and completion
    // are tracked by the install listener inside PlayInAppUpdateManager.
    val updateFlowLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.StartIntentSenderForResult(),
    ) { }

    val accent = when (state.status) {
        AppUpdateStatus.UPDATE_AVAILABLE, AppUpdateStatus.DOWNLOADED -> MaterialTheme.colorScheme.primary
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
                    UpdateProgressRow(
                        label = if (isPlayFlavor) locale.checkingPlayUpdates else locale.checkingGithub,
                    )
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
                        text = if (isPlayFlavor) {
                            locale.newVersionAvailableLabel
                        } else {
                            "${locale.newVersionAvailableLabel} ${state.latestVersion.orEmpty()}"
                        },
                        style = MaterialTheme.typography.titleSmall,
                        color = accent,
                        fontWeight = FontWeight.SemiBold,
                    )
                    // GitHub (sideload) builds render the release notes inline; Play owns
                    // the changelog for the play flavor, so notes are omitted there.
                    if (!isPlayFlavor && state.releaseNotes.isNotBlank()) {
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
                        text = if (isPlayFlavor) locale.updateNowButton else locale.downloadUpdateButton,
                        onClick = {
                            if (isPlayFlavor) {
                                playController?.startFlexibleUpdate(updateFlowLauncher)
                            } else if (!openAppUpdate(context, state)) {
                                Toast.makeText(
                                    context,
                                    locale.updateFailedLabel,
                                    Toast.LENGTH_SHORT,
                                ).show()
                            }
                        },
                    )
                }

                AppUpdateStatus.DOWNLOADING -> {
                    UpdateProgressRow(label = locale.updateDownloadingLabel)
                }

                AppUpdateStatus.DOWNLOADED -> {
                    Text(
                        text = locale.updateDownloadedLabel,
                        style = MaterialTheme.typography.titleSmall,
                        color = accent,
                        fontWeight = FontWeight.SemiBold,
                    )
                    UpdateSectionPrimaryButton(
                        text = locale.restartToUpdateButton,
                        onClick = { playController?.completeUpdate() },
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

@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
private fun UpdateProgressRow(label: String) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        CircularProgressIndicator(
            modifier = Modifier.size(18.dp),
            strokeWidth = 2.dp,
        )
        Text(
            text = label,
            style = MaterialTheme.typography.bodyMedium,
        )
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
