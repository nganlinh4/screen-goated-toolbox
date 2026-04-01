@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.ui.res.painterResource
import dev.screengoated.toolbox.mobile.R
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialShapes
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties

internal val DialogCloseMorphPair = ExpressiveMorphPair(
    from = MaterialShapes.Circle,
    to = MaterialShapes.Cookie4Sided,
)

@Composable
internal fun ExpressiveDialogSurface(
    title: String,
    @androidx.annotation.DrawableRes icon: Int,
    accent: Color,
    morphPair: ExpressiveMorphPair,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
    supporting: String? = null,
    widthFraction: Float = 0.94f,
    maxWidth: Dp = 560.dp,
    heightFraction: Float = 0.76f,
    maxHeight: Dp = 620.dp,
    fitContentHeight: Boolean = false,
    headerTrailing: @Composable (RowScope.() -> Unit)? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    val configuration = LocalConfiguration.current
    val isLandscape = configuration.screenWidthDp > configuration.screenHeightDp
    val availableHeight = configuration.screenHeightDp.dp - 32.dp
    val targetHeight = when {
        isLandscape -> minOf(maxHeight, availableHeight)
        else -> minOf(maxHeight, configuration.screenHeightDp.dp * heightFraction)
    }

    val bodyModifier = if (fitContentHeight) {
        Modifier
            .fillMaxWidth()
            .heightIn(max = targetHeight)
    } else {
        Modifier
            .fillMaxWidth()
            .height(targetHeight)
    }
    val columnModifier = if (fitContentHeight) {
        Modifier
            .fillMaxWidth()
            .widthIn(max = maxWidth)
    } else {
        Modifier
            .fillMaxWidth()
            .fillMaxHeight()
            .widthIn(max = maxWidth)
    }

    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false),
    ) {
        Card(
            modifier = modifier
                .fillMaxWidth(widthFraction)
                .padding(16.dp),
            shape = MaterialTheme.shapes.large,
            colors = CardDefaults.cardColors(
                containerColor = lerp(
                    MaterialTheme.colorScheme.surfaceContainerLow,
                    accent,
                    0.055f,
                ).copy(alpha = 0.985f),
                contentColor = MaterialTheme.colorScheme.onSurface,
            ),
        ) {
            BoxWithConstraints(
                modifier = bodyModifier
                    .padding(start = 20.dp, end = 14.dp, top = 16.dp, bottom = 18.dp),
            ) {
                Column(
                    modifier = columnModifier,
                    verticalArrangement = Arrangement.spacedBy(14.dp),
                ) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        MorphingShapeBadge(
                            morphPair = morphPair,
                            progress = 0.68f,
                            containerColor = lerp(
                                MaterialTheme.colorScheme.surfaceContainerHighest,
                                accent,
                                0.22f,
                            ),
                            modifier = Modifier.size(46.dp),
                        ) {
                            Icon(
                                painter = painterResource(icon),
                                contentDescription = null,
                                tint = accent,
                                modifier = Modifier.size(22.dp),
                            )
                        }
                        Column(
                            modifier = Modifier.weight(1f),
                            verticalArrangement = Arrangement.spacedBy(2.dp),
                        ) {
                            Text(
                                text = title,
                                style = MaterialTheme.typography.titleLarge,
                                color = MaterialTheme.colorScheme.onSurface,
                            )
                            if (!supporting.isNullOrBlank()) {
                                Text(
                                    text = supporting,
                                    style = MaterialTheme.typography.bodySmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                        }
                        if (headerTrailing != null) {
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(8.dp),
                                content = headerTrailing,
                            )
                        }
                        ExpressiveDialogCloseButton(
                            accent = accent,
                            onClick = onDismiss,
                        )
                    }
                    content()
                }
            }
        }
    }
}

@Composable
internal fun ExpressiveDialogSectionCard(
    accent: Color,
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    Card(
        modifier = modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.large,
        colors = CardDefaults.cardColors(
            containerColor = lerp(
                MaterialTheme.colorScheme.surfaceContainerLow,
                accent,
                0.045f,
            ),
            contentColor = MaterialTheme.colorScheme.onSurface,
        ),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
            content = content,
        )
    }
}

@Composable
internal fun ExpressiveDialogActionChip(
    text: String,
    accent: Color,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Box(
        modifier = modifier
            .clickable(onClick = onClick)
            .padding(vertical = 2.dp),
    ) {
        Card(
            shape = MaterialTheme.shapes.medium,
            colors = CardDefaults.cardColors(
                containerColor = accent.copy(alpha = 0.14f),
                contentColor = accent,
            ),
        ) {
            Text(
                text = text,
                style = MaterialTheme.typography.labelMediumEmphasized,
                modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
            )
        }
    }
}

@Composable
private fun ExpressiveDialogCloseButton(
    accent: Color,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val interactionSource = remember { MutableInteractionSource() }
    val pressed by interactionSource.collectIsPressedAsState()
    val morphProgress by animateFloatAsState(
        targetValue = if (pressed) 1f else 0f,
        animationSpec = spring(
            dampingRatio = androidx.compose.animation.core.Spring.DampingRatioMediumBouncy,
            stiffness = androidx.compose.animation.core.Spring.StiffnessMediumLow,
        ),
        label = "expressive-dialog-close-progress",
    )
    MorphingShapeBadge(
        morphPair = DialogCloseMorphPair,
        progress = morphProgress,
        containerColor = lerp(
            MaterialTheme.colorScheme.surfaceContainerHighest,
            accent,
            0.12f + (0.1f * morphProgress),
        ),
        modifier = modifier
            .size(38.dp)
            .clickable(
                interactionSource = interactionSource,
                indication = null,
                onClick = onClick,
            ),
    ) {
        Icon(
            painter = painterResource(R.drawable.ms_close),
            contentDescription = null,
            tint = lerp(
                MaterialTheme.colorScheme.onSurfaceVariant,
                accent,
                0.75f,
            ),
            modifier = Modifier.size(18.dp),
        )
    }
}
