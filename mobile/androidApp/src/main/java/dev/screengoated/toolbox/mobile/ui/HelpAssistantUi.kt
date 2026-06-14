@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import android.content.res.Configuration
import android.content.Intent
import android.provider.Settings
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.ui.res.painterResource
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.ui.i18n.uiLocalized
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialShapes
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.helpassistant.helpPlaceholder
import dev.screengoated.toolbox.mobile.service.helpassistant.HelpAssistantOverlayService
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

@Composable
internal fun HelpAssistantCard(
    locale: MobileLocaleText,
    modifier: Modifier = Modifier,
) {
    var showDialog by rememberSaveable { mutableStateOf(false) }

    ExpressiveSettingsCard(
        accent = MaterialTheme.colorScheme.primary,
        modifier = modifier
            .fillMaxWidth()
            .clickable { showDialog = true },
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(ShellSpacing.itemGap)) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                MorphingShapeBadge(
                    morphPair = ExpressiveMorphPair(MaterialShapes.Circle, MaterialShapes.Cookie4Sided),
                    progress = 0.54f,
                    containerColor = MaterialTheme.colorScheme.primary.copy(alpha = 0.18f),
                    modifier = Modifier.size(42.dp),
                ) {
                    Icon(
                        painter = painterResource(R.drawable.ms_auto_stories),
                        contentDescription = null,
                        modifier = Modifier.size(20.dp),
                        tint = MaterialTheme.colorScheme.primary,
                    )
                }
                Column {
                    Text(
                        text = locale.shellHelpLabel,
                        style = MaterialTheme.typography.titleSmall,
                        fontWeight = FontWeight.Bold,
                    )
                    Text(
                        text = locale.shellHelpDescription,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
    }

    if (showDialog) {
        HelpAssistantDialog(
            locale = locale,
            onDismiss = { showDialog = false },
        )
    }
}

@Composable
internal fun HelpAssistantActionButton(
    locale: MobileLocaleText,
    modifier: Modifier = Modifier,
) {
    var showDialog by rememberSaveable { mutableStateOf(false) }

    SettingsActionButton(
        text = locale.shellHelpLabel,
        icon = R.drawable.ms_auto_stories,
        onClick = { showDialog = true },
        modifier = modifier,
        morphStyle = SettingsActionMorphStyle.HELP,
    )

    if (showDialog) {
        HelpAssistantDialog(
            locale = locale,
            onDismiss = { showDialog = false },
        )
    }
}

@Composable
private fun HelpAssistantDialog(
    locale: MobileLocaleText,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    val configuration = LocalConfiguration.current
    val windowInfo = LocalWindowInfo.current
    val density = LocalDensity.current
    val windowWidth = with(density) { windowInfo.containerSize.width.toDp() }
    val windowHeight = with(density) { windowInfo.containerSize.height.toDp() }
    val uiLanguage = (context.applicationContext as SgtMobileApplication)
        .appContainer
        .repository
        .currentUiPreferences()
        .uiLanguage
    val isLandscape = configuration.orientation == Configuration.ORIENTATION_LANDSCAPE || windowWidth > windowHeight
    val compactLandscape = isLandscape && windowHeight <= 430.dp
    var question by rememberSaveable { mutableStateOf("") }
    var pendingOverlayQuestion by rememberSaveable { mutableStateOf<String?>(null) }
    val trimmedQuestion = question.trim()
    fun startHelpOverlay(questionText: String) {
        HelpAssistantOverlayService.start(
            context = context,
            question = questionText,
            uiLanguage = uiLanguage,
        )
        onDismiss()
    }
    val overlayPermissionLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.StartActivityForResult(),
    ) {
        val pendingQuestion = pendingOverlayQuestion
        pendingOverlayQuestion = null
        if (pendingQuestion != null && Settings.canDrawOverlays(context)) {
            startHelpOverlay(pendingQuestion)
        } else if (pendingQuestion != null) {
            Toast.makeText(
                context,
                context.uiLocalized().getString(R.string.help_assistant_overlay_permission_required),
                Toast.LENGTH_SHORT,
            ).show()
        }
    }

    ExpressiveDialogSurface(
        title = locale.helpAssistantTitle,
        icon = R.drawable.ms_auto_stories,
        accent = MaterialTheme.colorScheme.primary,
        morphPair = ExpressiveMorphPair(MaterialShapes.Circle, MaterialShapes.Cookie4Sided),
        onDismiss = onDismiss,
        supporting = if (compactLandscape) null else locale.helpAssistantHint,
        widthFraction = 0.92f,
        maxWidth = 620.dp,
        maxHeight = if (compactLandscape) 500.dp else 560.dp,
        fitContentHeight = true,
    ) {
        Column(
            modifier = Modifier.fillMaxWidth(),
            verticalArrangement = Arrangement.spacedBy(if (compactLandscape) 8.dp else 12.dp),
        ) {
            ExpressiveDialogSectionCard(
                accent = MaterialTheme.colorScheme.primary,
            ) {
                Text(
                    text = locale.helpAssistantQuestionLabel,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                OutlinedTextField(
                    value = question,
                    onValueChange = { question = it },
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(
                            min = if (compactLandscape) 72.dp else if (isLandscape) 96.dp else 120.dp,
                            max = if (compactLandscape) 104.dp else if (isLandscape) 140.dp else 180.dp,
                        ),
                    label = if (compactLandscape) null else ({ Text(locale.helpAssistantQuestionLabel) }),
                    placeholder = { Text(helpPlaceholder(locale)) },
                    minLines = if (compactLandscape) 2 else 3,
                )
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.End,
                ) {
                    ExpressiveSettingsButton(
                        text = locale.helpAssistantAskButton,
                        onClick = {
                            if (trimmedQuestion.isEmpty()) {
                                return@ExpressiveSettingsButton
                            }
                            if (Settings.canDrawOverlays(context)) {
                                startHelpOverlay(trimmedQuestion)
                            } else {
                                pendingOverlayQuestion = trimmedQuestion
                                overlayPermissionLauncher.launch(
                                    Intent(
                                        Settings.ACTION_MANAGE_OVERLAY_PERMISSION,
                                        "package:${context.packageName}".toUri(),
                                    ),
                                )
                            }
                        },
                        accent = MaterialTheme.colorScheme.primary,
                    )
                }
            }
        }
    }
}
