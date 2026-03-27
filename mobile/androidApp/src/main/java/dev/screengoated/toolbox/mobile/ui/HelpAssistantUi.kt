@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.ButtonGroupDefaults
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.HelpOutline
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialShapes
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.ToggleButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.helpassistant.HelpAssistantBucket
import dev.screengoated.toolbox.mobile.helpassistant.HelpAssistantMode
import dev.screengoated.toolbox.mobile.helpassistant.label
import dev.screengoated.toolbox.mobile.helpassistant.placeholder
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
                        imageVector = Icons.AutoMirrored.Rounded.HelpOutline,
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
        icon = Icons.AutoMirrored.Rounded.HelpOutline,
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
    val uiLanguage = (context.applicationContext as SgtMobileApplication)
        .appContainer
        .repository
        .currentUiPreferences()
        .uiLanguage
    val selectedBucket = HelpAssistantBucket.ANDROID
    var selectedMode by rememberSaveable { mutableStateOf(HelpAssistantMode.QUICK.wireId) }
    var question by rememberSaveable { mutableStateOf("") }
    val mode = HelpAssistantMode.entries.first { it.wireId == selectedMode }
    val trimmedQuestion = question.trim()

    ExpressiveDialogSurface(
        title = locale.helpAssistantTitle,
        icon = Icons.AutoMirrored.Rounded.HelpOutline,
        accent = MaterialTheme.colorScheme.primary,
        morphPair = ExpressiveMorphPair(MaterialShapes.Circle, MaterialShapes.Cookie4Sided),
        onDismiss = onDismiss,
        supporting = locale.helpAssistantHint,
        widthFraction = 0.92f,
        maxWidth = 620.dp,
        maxHeight = 560.dp,
        fitContentHeight = true,
    ) {
        Column(
            modifier = Modifier.fillMaxWidth(),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            ExpressiveDialogSectionCard(
                accent = MaterialTheme.colorScheme.primary,
            ) {
                Text(
                    text = locale.helpAssistantQuestionLabel,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(ButtonGroupDefaults.ConnectedSpaceBetween),
                ) {
                    HelpAssistantMode.entries.forEachIndexed { index, option ->
                        ToggleButton(
                            checked = mode == option,
                            onCheckedChange = { selectedMode = option.wireId },
                            shapes = when (index) {
                                0 -> ButtonGroupDefaults.connectedLeadingButtonShapes()
                                else -> ButtonGroupDefaults.connectedTrailingButtonShapes()
                            },
                            modifier = Modifier.semantics { role = Role.RadioButton },
                        ) {
                            Text(option.label(locale))
                        }
                    }
                }
                OutlinedTextField(
                    value = question,
                    onValueChange = { question = it },
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(min = 120.dp, max = 180.dp),
                    label = { Text(locale.helpAssistantQuestionLabel) },
                    placeholder = { Text(selectedBucket.placeholder(locale)) },
                    minLines = 3,
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
                            HelpAssistantOverlayService.start(
                                context = context,
                                bucket = selectedBucket,
                                mode = mode,
                                question = trimmedQuestion,
                                uiLanguage = uiLanguage,
                            )
                            onDismiss()
                        },
                        accent = MaterialTheme.colorScheme.primary,
                    )
                }
            }
        }
    }
}
