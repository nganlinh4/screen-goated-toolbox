@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.annotation.DrawableRes
import androidx.compose.ui.res.painterResource
import dev.screengoated.toolbox.mobile.R
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.luminance
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
internal fun HistoryInsetCard(
    accent: androidx.compose.ui.graphics.Color,
    modifier: Modifier = Modifier,
    content: @Composable BoxScope.() -> Unit,
) {
    Card(
        modifier = modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.large,
        colors = CardDefaults.cardColors(
            containerColor = lerp(
                MaterialTheme.colorScheme.surfaceContainerLow,
                accent,
                0.14f,
            ).copy(alpha = 0.98f),
            contentColor = MaterialTheme.colorScheme.onSurface,
        ),
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp, vertical = 14.dp),
            content = content,
        )
    }
}

@Composable
internal fun HistoryMetricChip(
    value: Int,
    modifier: Modifier = Modifier,
) {
    val accent = MaterialTheme.colorScheme.primary
    val contentColor = if (accent.luminance() > 0.52f) Color(0xFF221B12) else Color.White
    val normalized = ((value - 25f) / (200f - 25f)).coerceIn(0f, 1f)
    val (pair, pairProgress) = when {
        normalized < 0.25f -> ExpressiveMorphPair(MaterialShapes.Square, MaterialShapes.Cookie4Sided) to (normalized / 0.25f)
        normalized < 0.5f -> ExpressiveMorphPair(MaterialShapes.Cookie4Sided, MaterialShapes.Cookie6Sided) to ((normalized - 0.25f) / 0.25f)
        normalized < 0.75f -> ExpressiveMorphPair(MaterialShapes.Cookie6Sided, MaterialShapes.Cookie9Sided) to ((normalized - 0.5f) / 0.25f)
        else -> ExpressiveMorphPair(MaterialShapes.Cookie9Sided, MaterialShapes.Flower) to ((normalized - 0.75f) / 0.25f)
    }
    MorphingShapeBadge(
        morphPair = pair,
        progress = pairProgress,
        containerColor = accent,
        modifier = modifier,
    ) {
        Box(modifier = Modifier.padding(horizontal = 16.dp, vertical = 10.dp)) {
            Text(
                text = value.toString(),
                style = MaterialTheme.typography.labelMediumEmphasized,
                color = contentColor,
            )
        }
    }
}

@Composable
internal fun HistorySearchClearButton(
    onClick: () -> Unit,
    contentDescription: String,
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
        label = "history-search-clear",
    )
    val accent = MaterialTheme.colorScheme.error
    MorphingShapeBadge(
        morphPair = ExpressiveMorphPair(MaterialShapes.Circle, MaterialShapes.Cookie4Sided),
        progress = morphProgress,
        containerColor = lerp(
            MaterialTheme.colorScheme.surfaceContainerHighest,
            accent.copy(alpha = 0.92f),
            0.4f + (0.6f * morphProgress),
        ),
        modifier = modifier
            .size(38.dp)
            .clickable(
                interactionSource = interactionSource,
                indication = null,
                onClick = onClick,
            ),
    ) {
        androidx.compose.material3.Icon(
            painter = painterResource(R.drawable.ms_close),
            contentDescription = contentDescription,
            tint = Color.White,
            modifier = Modifier.size(16.dp),
        )
    }
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
            painter = painterResource(R.drawable.ms_history),
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
            R.drawable.ms_text_snippet,
        )
        HistoryType.IMAGE -> Triple(
            ExpressiveMorphPair(MaterialShapes.Circle, MaterialShapes.Flower),
            MaterialTheme.colorScheme.secondary,
            R.drawable.ms_image,
        )
        HistoryType.AUDIO -> Triple(
            ExpressiveMorphPair(MaterialShapes.Pill, MaterialShapes.Clover4Leaf),
            MaterialTheme.colorScheme.tertiary,
            R.drawable.ms_audio_file,
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
            painter = painterResource(icon),
            contentDescription = null,
            tint = accent,
            modifier = Modifier.size(16.dp),
        )
    }
}

@Composable
internal fun HistoryActionButton(
    text: String?,
    @DrawableRes icon: Int,
    role: HistoryActionRole,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    badgeRotationDegrees: Float = 0f,
    iconRotationDegrees: Float = 0f,
    contentDescription: String? = text,
    emphasis: Boolean = false,
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
        lerp(
            MaterialTheme.colorScheme.surfaceContainerHighest,
            accent,
            if (emphasis) 0.46f else 0.22f,
        ),
        morphProgress,
    )
    val cardColor = if (emphasis) {
        lerp(
            MaterialTheme.colorScheme.surfaceContainerLow,
            accent,
            if (pressed) 0.34f else 0.26f,
        )
    } else {
        lerp(
            MaterialTheme.colorScheme.surfaceContainerLow,
            accent,
            if (pressed) 0.16f else 0.1f,
        )
    }
    val iconTint = if (emphasis && accent.luminance() <= 0.52f) {
        Color.White
    } else {
        lerp(MaterialTheme.colorScheme.onSurfaceVariant, accent, 0.9f)
    }
    val textColor = if (emphasis && accent.luminance() <= 0.52f) {
        Color.White
    } else {
        MaterialTheme.colorScheme.onSurface
    }

    Card(
        modifier = modifier
            .clickable(
                interactionSource = interactionSource,
                indication = null,
                onClick = onClick,
            ),
        shape = MaterialTheme.shapes.medium,
        colors = CardDefaults.cardColors(
            containerColor = cardColor,
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
                modifier = Modifier
                    .size(34.dp)
                    .graphicsLayer(rotationZ = badgeRotationDegrees),
            ) {
                androidx.compose.material3.Icon(
                    painter = painterResource(icon),
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
                    color = textColor,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Spacer(Modifier.weight(1f))
            }
        }
    }
}
