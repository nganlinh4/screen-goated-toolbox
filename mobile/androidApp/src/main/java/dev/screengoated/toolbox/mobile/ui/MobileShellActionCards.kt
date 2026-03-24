@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.RestartAlt
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
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
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

@Composable
internal fun SettingsActionButton(
    text: String,
    icon: ImageVector,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    morphStyle: SettingsActionMorphStyle = SettingsActionMorphStyle.PRIORITY,
    destructive: Boolean = false,
) {
    val accent = if (destructive) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary
    val labelColor = MaterialTheme.colorScheme.onSurface
    val interactionSource = remember { MutableInteractionSource() }
    val pressed by interactionSource.collectIsPressedAsState()
    val morphProgress by animateFloatAsState(
        targetValue = if (pressed) 1f else 0f,
        animationSpec = spring(
            dampingRatio = androidx.compose.animation.core.Spring.DampingRatioMediumBouncy,
            stiffness = androidx.compose.animation.core.Spring.StiffnessMediumLow,
        ),
        label = "settings-action-morph",
    )
    ExpressiveSettingsCard(
        accent = accent,
        modifier = modifier,
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clickable(
                    interactionSource = interactionSource,
                    indication = null,
                    onClick = onClick,
                ),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ShellSpacing.itemGap),
        ) {
            MorphingShapeBadge(
                morphPair = morphStyle.pair,
                progress = morphProgress,
                containerColor = lerp(accent.copy(alpha = 0.18f), accent.copy(alpha = 0.28f), morphProgress),
                modifier = Modifier.size(44.dp),
            ) {
                Icon(
                    imageVector = icon,
                    contentDescription = null,
                    modifier = Modifier.size(18.dp),
                    tint = accent,
                )
            }
            Text(
                text = text,
                style = MaterialTheme.typography.labelLargeEmphasized,
                color = labelColor,
                maxLines = 2,
                modifier = Modifier.weight(1f),
            )
        }
    }
}

@Composable
internal fun OverlayOpacityCard(
    opacityPercent: Int,
    locale: MobileLocaleText,
    onOpacityChanged: (Int) -> Unit,
    modifier: Modifier = Modifier,
) {
    ExpressiveSettingsCard(
        modifier = modifier.fillMaxWidth(),
        accent = MaterialTheme.colorScheme.secondary,
    ) {
        Column(
            modifier = Modifier.fillMaxWidth(),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(ShellSpacing.itemGap),
            ) {
                Text(
                    text = locale.overlayOpacityLabel,
                    style = MaterialTheme.typography.titleMedium,
                    color = MaterialTheme.colorScheme.onSurface,
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
        morphStyle = SettingsActionMorphStyle.RESET,
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
