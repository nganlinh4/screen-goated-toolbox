@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.HelpOutline
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.helpassistant.HelpAssistantBucket
import dev.screengoated.toolbox.mobile.helpassistant.placeholder
import dev.screengoated.toolbox.mobile.service.helpassistant.HelpAssistantOverlayService
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

@Composable
internal fun HelpAssistantCard(
    locale: MobileLocaleText,
    modifier: Modifier = Modifier,
) {
    var showDialog by rememberSaveable { mutableStateOf(false) }

    Card(
        modifier = modifier
            .fillMaxWidth()
            .clickable { showDialog = true },
        shape = MaterialTheme.shapes.large,
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
        ),
    ) {
        Column(
            modifier = Modifier.padding(ShellSpacing.innerPad),
            verticalArrangement = Arrangement.spacedBy(ShellSpacing.itemGap),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Icon(
                    imageVector = Icons.AutoMirrored.Rounded.HelpOutline,
                    contentDescription = null,
                    modifier = Modifier.size(24.dp),
                    tint = MaterialTheme.colorScheme.primary,
                )
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

    FilledTonalButton(
        onClick = { showDialog = true },
        modifier = modifier,
        shape = MaterialTheme.shapes.large,
    ) {
        Icon(
            imageVector = Icons.AutoMirrored.Rounded.HelpOutline,
            contentDescription = null,
            modifier = Modifier.size(18.dp),
        )
        Spacer(modifier = Modifier.padding(start = androidx.compose.material3.ButtonDefaults.IconSpacing))
        Text(
            text = locale.shellHelpLabel,
            style = MaterialTheme.typography.labelLargeEmphasized,
        )
    }

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
    var question by rememberSaveable { mutableStateOf("") }
    val trimmedQuestion = question.trim()

    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false),
    ) {
        Card(
            modifier = Modifier
                .fillMaxWidth(0.94f)
                .widthIn(max = 560.dp)
                .padding(16.dp),
            shape = MaterialTheme.shapes.small,
            colors = CardDefaults.cardColors(
                containerColor = MaterialTheme.colorScheme.surface,
            ),
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(20.dp)
                    .verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(14.dp),
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Text(
                        text = locale.helpAssistantTitle,
                        style = MaterialTheme.typography.titleLarge,
                        fontWeight = FontWeight.SemiBold,
                    )
                    IconButton(onClick = onDismiss) {
                        Icon(Icons.Rounded.Close, contentDescription = null)
                    }
                }

                Text(
                    text = locale.helpAssistantHint,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )

                OutlinedTextField(
                    value = question,
                    onValueChange = { question = it },
                    modifier = Modifier.fillMaxWidth(),
                    label = { Text(locale.helpAssistantQuestionLabel) },
                    placeholder = { Text(selectedBucket.placeholder(locale)) },
                    minLines = 3,
                )

                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.End,
                ) {
                    Button(
                        enabled = trimmedQuestion.isNotEmpty(),
                        onClick = {
                            HelpAssistantOverlayService.start(
                                context = context,
                                bucket = selectedBucket,
                                question = trimmedQuestion,
                                uiLanguage = uiLanguage,
                            )
                            onDismiss()
                        },
                    ) {
                        Text(locale.helpAssistantAskButton)
                    }
                }
            }
        }
    }
}
