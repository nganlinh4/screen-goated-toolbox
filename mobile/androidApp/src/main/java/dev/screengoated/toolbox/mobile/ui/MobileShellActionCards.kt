@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.RestartAlt
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

@Composable
internal fun SettingsActionButton(
    text: String,
    icon: ImageVector,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    destructive: Boolean = false,
) {
    FilledTonalButton(
        onClick = onClick,
        modifier = modifier,
        shape = MaterialTheme.shapes.large,
        colors = if (destructive) {
            ButtonDefaults.filledTonalButtonColors(
                containerColor = MaterialTheme.colorScheme.errorContainer,
                contentColor = MaterialTheme.colorScheme.onErrorContainer,
            )
        } else {
            ButtonDefaults.filledTonalButtonColors()
        },
    ) {
        Icon(
            imageVector = icon,
            contentDescription = null,
            modifier = Modifier.size(18.dp),
        )
        Spacer(modifier = Modifier.padding(start = ButtonDefaults.IconSpacing))
        Text(
            text = text,
            style = MaterialTheme.typography.labelLargeEmphasized,
            maxLines = 1,
        )
    }
}

@Composable
internal fun OverlayOpacityCard(
    opacityPercent: Int,
    locale: MobileLocaleText,
    onOpacityChanged: (Int) -> Unit,
    modifier: Modifier = Modifier,
) {
    Card(
        modifier = modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.large,
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
        ),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = ShellSpacing.innerPad, vertical = 14.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(ShellSpacing.itemGap),
            ) {
                Text(
                    text = locale.overlayOpacityLabel,
                    style = MaterialTheme.typography.titleMedium,
                    modifier = Modifier.weight(1f),
                )
                Text(
                    text = "$opacityPercent%",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            Slider(
                value = opacityPercent.toFloat(),
                onValueChange = { onOpacityChanged(it.toInt()) },
                valueRange = 10f..100f,
            )
        }
    }
}

@Composable
internal fun ResetDefaultsActionButton(
    locale: MobileLocaleText,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var showConfirm by remember { mutableStateOf(false) }
    val context = androidx.compose.ui.platform.LocalContext.current
    val doneMsg = when {
        locale.resetDefaultsButton.contains("Khôi") -> "Đã khôi phục mặc định"
        locale.resetDefaultsButton.contains("복원") -> "기본값으로 복원됨"
        else -> "Defaults restored"
    }

    SettingsActionButton(
        text = locale.resetDefaultsButton,
        icon = Icons.Rounded.RestartAlt,
        onClick = { showConfirm = true },
        modifier = modifier,
        destructive = true,
    )

    if (showConfirm) {
        AlertDialog(
            onDismissRequest = { showConfirm = false },
            title = { Text(locale.resetDefaultsConfirmTitle) },
            text = { Text(locale.resetDefaultsConfirmMessage) },
            confirmButton = {
                TextButton(
                    onClick = {
                        showConfirm = false
                        onClick()
                        android.widget.Toast.makeText(context, doneMsg, android.widget.Toast.LENGTH_SHORT).show()
                    },
                ) {
                    Text(locale.resetDefaultsAction)
                }
            },
            dismissButton = {
                TextButton(onClick = { showConfirm = false }) {
                    Text(locale.closeLabel)
                }
            },
        )
    }
}
