@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.TextSnippet
import androidx.compose.material.icons.rounded.GraphicEq
import androidx.compose.material.icons.rounded.History
import androidx.compose.material.icons.rounded.Image
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialShapes
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.history.HistoryType

internal enum class HistoryActionRole(
    val morphPair: ExpressiveMorphPair,
) {
    FOLDER(ExpressiveMorphPair(MaterialShapes.Arch, MaterialShapes.Cookie7Sided)),
    COPY(ExpressiveMorphPair(MaterialShapes.Square, MaterialShapes.Cookie4Sided)),
    OPEN(ExpressiveMorphPair(MaterialShapes.Pill, MaterialShapes.Arrow)),
    DELETE(ExpressiveMorphPair(MaterialShapes.Slanted, MaterialShapes.Pentagon)),
}

@Composable
internal fun HistorySectionHeroBadge(
    modifier: Modifier = Modifier,
) {
    MorphingShapeBadge(
        morphPair = ExpressiveMorphPair(MaterialShapes.Circle, MaterialShapes.Cookie7Sided),
        progress = 0.68f,
        containerColor = lerp(
            MaterialTheme.colorScheme.primaryContainer,
            MaterialTheme.colorScheme.primary,
            0.16f,
        ),
        modifier = modifier,
    ) {
        androidx.compose.material3.Icon(
            imageVector = Icons.Rounded.History,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.primary,
            modifier = Modifier.size(20.dp),
        )
    }
}

@Composable
internal fun HistoryTypeBadge(
    type: HistoryType,
    modifier: Modifier = Modifier,
) {
    val (pair, accent, icon) = when (type) {
        HistoryType.TEXT -> Triple(
            ExpressiveMorphPair(MaterialShapes.Square, MaterialShapes.Gem),
            MaterialTheme.colorScheme.primary,
            Icons.AutoMirrored.Rounded.TextSnippet,
        )
        HistoryType.IMAGE -> Triple(
            ExpressiveMorphPair(MaterialShapes.Circle, MaterialShapes.Flower),
            MaterialTheme.colorScheme.secondary,
            Icons.Rounded.Image,
        )
        HistoryType.AUDIO -> Triple(
            ExpressiveMorphPair(MaterialShapes.Pill, MaterialShapes.Clover4Leaf),
            MaterialTheme.colorScheme.tertiary,
            Icons.Rounded.GraphicEq,
        )
    }
    MorphingShapeBadge(
        morphPair = pair,
        progress = 0.72f,
        containerColor = lerp(
            MaterialTheme.colorScheme.surfaceContainerHighest,
            accent,
            0.18f,
        ),
        modifier = modifier,
    ) {
        androidx.compose.material3.Icon(
            imageVector = icon,
            contentDescription = null,
            tint = accent,
            modifier = Modifier.size(16.dp),
        )
    }
}

@Composable
internal fun HistoryActionButton(
    text: String?,
    icon: ImageVector,
    role: HistoryActionRole,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    iconRotationDegrees: Float = 0f,
    contentDescription: String? = text,
) {
    val accent = when (role) {
        HistoryActionRole.FOLDER -> MaterialTheme.colorScheme.primary
        HistoryActionRole.COPY -> MaterialTheme.colorScheme.primary
        HistoryActionRole.OPEN -> MaterialTheme.colorScheme.tertiary
        HistoryActionRole.DELETE -> MaterialTheme.colorScheme.error
    }
    val interactionSource = remember { MutableInteractionSource() }
    val pressed by interactionSource.collectIsPressedAsState()
    val morphProgress by animateFloatAsState(
        targetValue = if (pressed) 1f else 0f,
        animationSpec = spring(
            dampingRatio = androidx.compose.animation.core.Spring.DampingRatioMediumBouncy,
            stiffness = androidx.compose.animation.core.Spring.StiffnessMediumLow,
        ),
        label = "history-action-progress-${text ?: role.name.lowercase()}",
    )
    val containerColor = lerp(
        lerp(MaterialTheme.colorScheme.surfaceContainerHighest, accent, 0.12f),
        lerp(MaterialTheme.colorScheme.surfaceContainerHighest, accent, 0.22f),
        morphProgress,
    )
    val iconTint = lerp(MaterialTheme.colorScheme.onSurfaceVariant, accent, 0.9f)

    Card(
        modifier = modifier
            .clickable(
                interactionSource = interactionSource,
                indication = null,
                onClick = onClick,
            ),
        shape = MaterialTheme.shapes.medium,
        colors = CardDefaults.cardColors(
            containerColor = lerp(
                MaterialTheme.colorScheme.surfaceContainerLow,
                accent,
                if (pressed) 0.16f else 0.1f,
            ),
        ),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 12.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = if (text.isNullOrBlank()) {
                Arrangement.Center
            } else {
                Arrangement.spacedBy(10.dp)
            },
        ) {
            MorphingShapeBadge(
                morphPair = role.morphPair,
                progress = morphProgress,
                containerColor = containerColor,
                modifier = Modifier.size(34.dp),
            ) {
                androidx.compose.material3.Icon(
                    imageVector = icon,
                    contentDescription = contentDescription,
                    tint = iconTint,
                    modifier = Modifier
                        .size(16.dp)
                        .graphicsLayer(rotationZ = iconRotationDegrees),
                )
            }
            if (!text.isNullOrBlank()) {
                Text(
                    text = text,
                    style = MaterialTheme.typography.labelMediumEmphasized,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Spacer(Modifier.weight(1f))
            }
        }
    }
}
