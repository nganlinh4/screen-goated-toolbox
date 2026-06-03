@file:OptIn(
    androidx.compose.material3.ExperimentalMaterial3Api::class,
    androidx.compose.material3.ExperimentalMaterial3ExpressiveApi::class,
)

package dev.screengoated.toolbox.mobile.translationgummy

import android.Manifest
import android.content.pm.PackageManager
import androidx.compose.animation.animateColorAsState
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.res.painterResource
import dev.screengoated.toolbox.mobile.R
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.asAndroidPath
import androidx.compose.ui.graphics.asComposePath
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.material3.MaterialShapes
import androidx.graphics.shapes.Morph
import androidx.graphics.shapes.RoundedPolygon
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText


// Language card + status/waveform/morph widgets extracted from TranslationGummyScreen.
@Composable
internal fun TranslationGummyLanguageCard(
    number: String,
    title: String,
    profile: TranslationGummyLanguageProfile,
    languagePlaceholder: String,
    accentPlaceholder: String,
    tonePlaceholder: String,
    accent: Color,
    onChanged: ((TranslationGummyLanguageProfile) -> Unit)? = null,
    onLanguageChanged: (String) -> Unit,
    onAccentChanged: (String) -> Unit,
    onToneChanged: (String) -> Unit,
) {
    val surfaceLow = MaterialTheme.colorScheme.surfaceContainerLow
    Card(
        shape = MaterialTheme.shapes.large,
        colors = CardDefaults.cardColors(
            containerColor = lerp(surfaceLow, accent, 0.05f),
        ),
    ) {
        Column(
            modifier = Modifier.fillMaxWidth().padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            // Row 1: Number badge + title + language input
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                // Number badge — soft Bun shape
                MorphBadge(
                    from = MaterialShapes.SoftBoom,
                    to = MaterialShapes.SoftBoom,
                    progress = 0f,
                    containerColor = accent.copy(alpha = 0.15f),
                    modifier = Modifier.size(28.dp),
                ) {
                    Text(
                        number,
                        style = MaterialTheme.typography.labelMedium,
                        fontWeight = FontWeight.Bold,
                        color = accent,
                    )
                }
                Text(
                    title,
                    style = MaterialTheme.typography.titleSmall,
                    color = accent,
                    fontWeight = FontWeight.SemiBold,
                )
                OutlinedTextField(
                    value = profile.language,
                    onValueChange = onLanguageChanged,
                    placeholder = { Text(languagePlaceholder, style = MaterialTheme.typography.bodySmall) },
                    modifier = Modifier.weight(1f).defaultMinSize(minHeight = 44.dp),
                    singleLine = true,
                    textStyle = MaterialTheme.typography.bodyMedium,
                    shape = MaterialTheme.shapes.large,
                    colors = translationGummyTextFieldColors(accent),
                )
            }
            // Row 2: Accent + Tone
            Row(
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                OutlinedTextField(
                    value = profile.accent,
                    onValueChange = onAccentChanged,
                    placeholder = { Text(accentPlaceholder, style = MaterialTheme.typography.bodySmall) },
                    modifier = Modifier.weight(1f).defaultMinSize(minHeight = 44.dp),
                    singleLine = true,
                    textStyle = MaterialTheme.typography.bodySmall,
                    shape = MaterialTheme.shapes.large,
                    colors = translationGummyTextFieldColors(accent),
                )
                OutlinedTextField(
                    value = profile.tone,
                    onValueChange = onToneChanged,
                    placeholder = { Text(tonePlaceholder, style = MaterialTheme.typography.bodySmall) },
                    modifier = Modifier.weight(1f).defaultMinSize(minHeight = 44.dp),
                    singleLine = true,
                    textStyle = MaterialTheme.typography.bodySmall,
                    shape = MaterialTheme.shapes.large,
                    colors = translationGummyTextFieldColors(accent),
                )
            }
        }
    }
}

@Composable
internal fun translationGummyTextFieldColors(accent: Color): TextFieldColors {
    return OutlinedTextFieldDefaults.colors(
        focusedContainerColor = accent.copy(alpha = 0.08f),
        unfocusedContainerColor = accent.copy(alpha = 0.04f),
        focusedBorderColor = accent.copy(alpha = 0.6f),
        unfocusedBorderColor = accent.copy(alpha = 0.2f),
        cursorColor = accent,
    )
}

// ── Transcript ──

internal sealed class TranscriptEntry {
    data class Pair(
        val id: Long,
        val input: String,
        val output: String,
        val lang: String,
        val placement: TranscriptBubblePlacement,
    ) : TranscriptEntry()
    data class Sep(val id: Long, val time: String) : TranscriptEntry()
}

internal enum class TranscriptBubblePlacement {
    CENTER,
    LEFT,
    RIGHT,
}


@Composable
internal fun StatusDot(connectionState: TranslationGummyConnectionState) {
    val color = when (connectionState) {
        TranslationGummyConnectionState.READY -> Color(0xFF4CAF50)
        TranslationGummyConnectionState.CONNECTING -> CoralAccent
        TranslationGummyConnectionState.RECONNECTING -> Color(0xFFFFC107)
        TranslationGummyConnectionState.ERROR -> Color(0xFFF44336)
        else -> MaterialTheme.colorScheme.outlineVariant
    }
    Canvas(modifier = Modifier.size(8.dp)) {
        drawCircle(color = color)
        if (connectionState == TranslationGummyConnectionState.READY) {
            drawCircle(color = color.copy(alpha = 0.3f), radius = size.minDimension * 0.8f)
        }
    }
}

// ── Compact Waveform ──

private const val WF_NUM_BARS = 22

@Composable
internal fun CompactWaveform(
    connectionState: TranslationGummyConnectionState,
    level: Float,
    accent: Color,
    modifier: Modifier = Modifier,
) {
    // Real RMS-driven scrolling bars (matches Windows recording indicator pattern)
    val barHeights = remember { FloatArray(WF_NUM_BARS + 2) }
    var scrollProgress by remember { mutableFloatStateOf(0f) }
    var lastFrameNanos by remember { mutableLongStateOf(0L) }

    val gradient = Brush.verticalGradient(listOf(accent, accent.copy(alpha = 0.85f), accent.copy(alpha = 0.6f)))

    val displayLevel = when (connectionState) {
        TranslationGummyConnectionState.READY -> level.coerceAtLeast(0.02f)
        TranslationGummyConnectionState.CONNECTING -> 0.10f
        TranslationGummyConnectionState.RECONNECTING -> 0.08f
        else -> 0.01f
    }

    Canvas(modifier = modifier) {
        val now = System.nanoTime()
        val dt = if (lastFrameNanos == 0L) 0.016f else ((now - lastFrameNanos) / 1_000_000_000f).coerceAtMost(0.05f)
        lastFrameNanos = now

        // Scroll bars left-to-right
        scrollProgress += dt / 0.15f
        val barWidth = 3.dp.toPx()
        val barGap = 2.dp.toPx()
        val barSpacing = barWidth + barGap
        val fadeWidth = 10.dp.toPx()
        val minH = 3.dp.toPx()
        val maxH = size.height - 2.dp.toPx()

        while (scrollProgress >= 1f) {
            scrollProgress -= 1f
            // Shift bars left, push new bar with real RMS level
            for (i in 0 until barHeights.size - 1) {
                barHeights[i] = barHeights[i + 1]
            }
            val newH = (displayLevel * maxH * 4f + minH).coerceIn(minH, maxH)
            barHeights[barHeights.size - 1] = newH
        }

        val pixelOffset = scrollProgress * barSpacing

        for (i in barHeights.indices) {
            val h = barHeights[i]
            val x = i * barSpacing - pixelOffset
            if (x + barWidth < 0f || x > size.width) continue

            val leftDist = x.coerceAtLeast(0f)
            val rightDist = (size.width - x - barWidth).coerceAtLeast(0f)
            val alpha = (minOf(leftDist, rightDist) / fadeWidth).coerceIn(0f, 1f)

            if (alpha > 0.01f && h > 0.5f) {
                drawRoundRect(
                    brush = gradient,
                    topLeft = Offset(x, (size.height - h) / 2f),
                    size = Size(barWidth, h),
                    cornerRadius = CornerRadius(barWidth / 2f),
                    alpha = alpha,
                )
            }
        }
    }
}

// ── MorphBadge (local M3E shape badge) ──

@Composable
internal fun MorphBadge(
    from: RoundedPolygon,
    to: RoundedPolygon,
    progress: Float,
    containerColor: Color,
    modifier: Modifier = Modifier,
    content: @Composable BoxScope.() -> Unit,
) {
    val morph = remember(from, to) { Morph(from, to) }
    Box(
        modifier = modifier.drawWithCache {
            val path = morphToPath(morph, progress, size)
            onDrawBehind { drawPath(path, containerColor) }
        },
        contentAlignment = Alignment.Center,
        content = content,
    )
}

internal fun morphToPath(morph: Morph, progress: Float, size: Size): androidx.compose.ui.graphics.Path {
    val composePath = morph.toPath(progress)
    val aPath = composePath.asAndroidPath()
    val bounds = android.graphics.RectF()
    aPath.computeBounds(bounds, true)
    val pw = maxOf(bounds.width(), 1f)
    val ph = maxOf(bounds.height(), 1f)
    val scale = minOf(size.width / pw, size.height / ph) * 0.9f
    val matrix = android.graphics.Matrix()
    matrix.postTranslate(-bounds.centerX(), -bounds.centerY())
    matrix.postScale(scale, scale)
    matrix.postTranslate(size.width / 2f, size.height / 2f)
    aPath.transform(matrix)
    return aPath.asComposePath()
}

