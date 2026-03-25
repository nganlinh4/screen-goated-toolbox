@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Visibility
import androidx.compose.material.icons.rounded.VisibilityOff
import androidx.compose.material3.MaterialShapes
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.graphics.asComposePath
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.unit.dp
import androidx.graphics.shapes.Morph
import androidx.graphics.shapes.RoundedPolygon
import androidx.graphics.shapes.toPath
import kotlin.math.max
import kotlin.math.min

internal data class ExpressiveMorphPair(
    val from: RoundedPolygon,
    val to: RoundedPolygon,
)

internal enum class SettingsActionMorphStyle(
    val pair: ExpressiveMorphPair,
) {
    PRIORITY(ExpressiveMorphPair(MaterialShapes.Square, MaterialShapes.Cookie6Sided)),
    STATS(ExpressiveMorphPair(MaterialShapes.Oval, MaterialShapes.Gem)),
    HELP(ExpressiveMorphPair(MaterialShapes.Bun, MaterialShapes.Flower)),
    RESET(ExpressiveMorphPair(MaterialShapes.Slanted, MaterialShapes.Pentagon)),
}

private val VisibilityToggleMorphPair = ExpressiveMorphPair(
    from = MaterialShapes.Circle,
    to = MaterialShapes.PuffyDiamond,
)

@Composable
internal fun ExpressiveSettingsCard(
    accent: Color,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    Card(
        modifier = modifier,
        shape = MaterialTheme.shapes.large,
        colors = CardDefaults.cardColors(
            containerColor = lerp(
                MaterialTheme.colorScheme.surfaceContainerLow,
                accent,
                0.035f,
            ).copy(alpha = 0.97f),
            contentColor = MaterialTheme.colorScheme.onSurface,
        ),
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .padding(ShellSpacing.innerPad),
        ) { content() }
    }
}

@Composable
internal fun ExpressiveSettingsHeader(
    title: String,
    icon: ImageVector,
    accent: Color,
    supporting: String? = null,
) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ShellSpacing.itemGap),
    ) {
        Box(
            modifier = Modifier
                .size(40.dp)
                .background(accent.copy(alpha = 0.18f), MaterialTheme.shapes.medium),
            contentAlignment = Alignment.Center,
        ) {
            GradientMaskedIcon(
                imageVector = icon,
                brush = Brush.linearGradient(listOf(accent, MaterialTheme.colorScheme.primary)),
                modifier = Modifier.size(20.dp),
            )
        }
        Column(
            modifier = Modifier.weight(1f),
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            Text(text = title, style = MaterialTheme.typography.titleMedium)
            if (supporting != null) {
                Text(
                    text = supporting,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

@Composable
internal fun ExpressiveSettingsButton(
    text: String,
    onClick: () -> Unit,
    accent: Color,
    modifier: Modifier = Modifier,
) {
    FilledTonalButton(
        onClick = onClick,
        modifier = modifier,
        shape = MaterialTheme.shapes.medium,
    ) {
        Text(
            text = text,
            color = accent,
            style = MaterialTheme.typography.labelMediumEmphasized,
        )
    }
}

@Composable
internal fun ExpressiveSettingsInsetCard(
    accent: Color,
    modifier: Modifier = Modifier,
    horizontalPadding: androidx.compose.ui.unit.Dp = 12.dp,
    verticalPadding: androidx.compose.ui.unit.Dp = 12.dp,
    content: @Composable BoxScope.() -> Unit,
) {
    Card(
        modifier = modifier,
        shape = MaterialTheme.shapes.medium,
        colors = CardDefaults.cardColors(
            containerColor = lerp(
                MaterialTheme.colorScheme.surfaceContainerLow,
                accent,
                0.11f,
            ).copy(alpha = 0.98f),
            contentColor = MaterialTheme.colorScheme.onSurface,
        ),
        border = BorderStroke(
            width = 1.dp,
            color = accent.copy(alpha = 0.22f),
        ),
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = horizontalPadding, vertical = verticalPadding),
            content = content,
        )
    }
}

@Composable
internal fun ExpressiveProviderToggleChip(
    text: String,
    icon: ImageVector,
    accent: Color,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
) {
    val interactionSource = remember { MutableInteractionSource() }
    val pressed by interactionSource.collectIsPressedAsState()
    val selectedContent = if (accent.luminance() > 0.52f) {
        Color(0xFF221B12)
    } else {
        Color.White
    }
    val emphasis by animateFloatAsState(
        targetValue = when {
            checked -> 1f
            pressed -> 0.42f
            else -> 0f
        },
        animationSpec = spring(
            dampingRatio = androidx.compose.animation.core.Spring.DampingRatioMediumBouncy,
            stiffness = androidx.compose.animation.core.Spring.StiffnessMediumLow,
        ),
        label = "provider-chip-$text",
    )
    val containerColor = lerp(
        MaterialTheme.colorScheme.surfaceContainerHigh,
        accent,
        if (checked) 0.88f else 0.18f + (0.22f * emphasis),
    )
    val borderColor = lerp(
        MaterialTheme.colorScheme.outline.copy(alpha = 0.18f),
        accent.copy(alpha = if (checked) 0.88f else 0.44f),
        0.35f + (0.65f * emphasis),
    )
    val badgeColor = lerp(
        MaterialTheme.colorScheme.surfaceContainerHighest,
        if (checked) {
            selectedContent.copy(alpha = 0.22f)
        } else {
            accent.copy(alpha = 0.2f)
        },
        0.38f + (0.55f * emphasis),
    )
    val contentColor = if (checked) {
        selectedContent
    } else {
        lerp(MaterialTheme.colorScheme.onSurfaceVariant, accent, 0.8f)
    }
    val morphProgress by animateFloatAsState(
        targetValue = if (checked) 0.96f else 0.28f,
        animationSpec = spring(
            dampingRatio = androidx.compose.animation.core.Spring.DampingRatioMediumBouncy,
            stiffness = androidx.compose.animation.core.Spring.StiffnessMediumLow,
        ),
        label = "provider-chip-morph-$text",
    )

    Card(
        modifier = modifier
            .graphicsLayer {
                scaleX = 0.982f + (0.018f * emphasis)
                scaleY = 0.982f + (0.018f * emphasis)
            }
            .clickable(
                interactionSource = interactionSource,
                indication = null,
                onClick = { onCheckedChange(!checked) },
            ),
        shape = MaterialTheme.shapes.extraLarge,
        colors = CardDefaults.cardColors(
            containerColor = containerColor,
            contentColor = contentColor,
        ),
        border = BorderStroke(1.dp, borderColor),
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 9.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            MorphingShapeBadge(
                morphPair = ExpressiveMorphPair(MaterialShapes.Circle, MaterialShapes.Cookie6Sided),
                progress = morphProgress,
                containerColor = badgeColor,
                modifier = Modifier.size(30.dp),
            ) {
                Icon(
                    imageVector = icon,
                    contentDescription = null,
                    tint = contentColor,
                    modifier = Modifier.size(15.dp),
                )
            }
            Text(
                text = text,
                color = contentColor,
                style = MaterialTheme.typography.labelMediumEmphasized,
            )
        }
    }
}

@Composable
internal fun ExpressiveProviderExpandSwitch(
    checked: Boolean,
    accent: Color,
    onCheckedChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
) {
    Switch(
        checked = checked,
        onCheckedChange = onCheckedChange,
        modifier = modifier.graphicsLayer(
            scaleX = 0.78f,
            scaleY = 0.78f,
        ),
        colors = SwitchDefaults.colors(
            checkedThumbColor = if (accent.luminance() > 0.52f) Color(0xFF221B12) else Color.White,
            checkedTrackColor = accent,
            checkedBorderColor = accent,
            uncheckedThumbColor = MaterialTheme.colorScheme.outline,
            uncheckedTrackColor = accent.copy(alpha = 0.18f),
            uncheckedBorderColor = accent.copy(alpha = 0.34f),
        ),
    )
}

@Composable
internal fun MorphingShapeBadge(
    morphPair: ExpressiveMorphPair,
    progress: Float,
    containerColor: Color,
    modifier: Modifier = Modifier,
    insetFraction: Float = 0.9f,
    contentAlignment: Alignment = Alignment.Center,
    content: @Composable BoxScope.() -> Unit,
) {
    val morph = remember(morphPair.from, morphPair.to) { Morph(morphPair.from, morphPair.to) }
    Box(
        modifier = modifier.drawWithCache {
            val path = settingsMorphPath(
                morph = morph,
                progress = progress,
                size = size,
                insetFraction = insetFraction,
            )
            onDrawBehind {
                drawPath(path = path, color = containerColor)
            }
        },
        contentAlignment = contentAlignment,
    ) {
        content()
    }
}

@Composable
internal fun MorphingVisibilityToggleButton(
    visible: Boolean,
    accent: Color,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val interactionSource = remember { MutableInteractionSource() }
    val pressed by interactionSource.collectIsPressedAsState()
    val visibleProgress by animateFloatAsState(
        targetValue = if (visible) 1f else 0f,
        animationSpec = spring(
            dampingRatio = androidx.compose.animation.core.Spring.DampingRatioMediumBouncy,
            stiffness = androidx.compose.animation.core.Spring.StiffnessMediumLow,
        ),
        label = "settings-visibility-progress",
    )
    val pressScale by animateFloatAsState(
        targetValue = if (pressed) 1.08f else 1f,
        animationSpec = spring(
            dampingRatio = androidx.compose.animation.core.Spring.DampingRatioMediumBouncy,
            stiffness = androidx.compose.animation.core.Spring.StiffnessMediumLow,
        ),
        label = "settings-visibility-scale",
    )
    val containerColor = lerp(
        MaterialTheme.colorScheme.surfaceContainerHighest,
        accent.copy(alpha = 0.28f),
        visibleProgress,
    )
    val iconTint = lerp(
        MaterialTheme.colorScheme.onSurfaceVariant,
        accent,
        visibleProgress,
    )

    MorphingShapeBadge(
        morphPair = VisibilityToggleMorphPair,
        progress = visibleProgress,
        containerColor = containerColor,
        modifier = modifier
            .graphicsLayer {
                scaleX = pressScale
                scaleY = pressScale
            }
            .clickable(
                interactionSource = interactionSource,
                indication = null,
                onClick = onClick,
            ),
    ) {
        Icon(
            imageVector = if (visible) Icons.Rounded.VisibilityOff else Icons.Rounded.Visibility,
            contentDescription = null,
            tint = iconTint,
            modifier = Modifier.padding(8.dp),
        )
    }
}

private fun settingsMorphPath(
    morph: Morph,
    progress: Float,
    size: Size,
    insetFraction: Float,
): Path {
    val androidPath = morph.toPath(progress, android.graphics.Path())
    val bounds = android.graphics.RectF()
    androidPath.computeBounds(bounds, true)
    val pathWidth = max(bounds.width(), 1f)
    val pathHeight = max(bounds.height(), 1f)
    val scale = min(size.width / pathWidth, size.height / pathHeight) * insetFraction
    val matrix = android.graphics.Matrix().apply {
        postTranslate(-bounds.centerX(), -bounds.centerY())
        postScale(scale, scale)
        postTranslate(size.width / 2f, size.height / 2f)
    }
    androidPath.transform(matrix)
    return androidPath.asComposePath()
}

internal fun providerAccent(label: String, colors: androidx.compose.material3.ColorScheme): Color = when (label) {
    "Gemini" -> colors.primary
    "Cerebras" -> Color(0xFFFF7043)
    "Groq" -> Color(0xFFFFB300)
    "OpenRouter" -> colors.tertiary
    "Ollama" -> colors.secondary
    else -> colors.primary
}
