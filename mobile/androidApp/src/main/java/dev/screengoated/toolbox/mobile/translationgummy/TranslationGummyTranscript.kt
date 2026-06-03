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


// Transcript grouping + chat-bubble composables extracted from TranslationGummyScreen.
internal fun groupEntries(items: List<TranslationGummyTranscriptItem>): List<TranscriptEntry> {
    val entries = mutableListOf<TranscriptEntry>()
    var i = 0
    while (i < items.size) {
        val item = items[i]
        if (item.role == TranslationGummyTranscriptRole.SEPARATOR) {
            entries += TranscriptEntry.Sep(item.id, item.text)
            i++
            continue
        }
        if (item.role == TranslationGummyTranscriptRole.INPUT) {
            val next = items.getOrNull(i + 1)
            if (next != null && next.role == TranslationGummyTranscriptRole.OUTPUT) {
                entries += TranscriptEntry.Pair(item.id, item.text, next.text, next.lang, TranscriptBubblePlacement.CENTER)
                i += 2
            } else {
                entries += TranscriptEntry.Pair(item.id, item.text, "", "", TranscriptBubblePlacement.CENTER)
                i++
            }
        } else {
            entries += TranscriptEntry.Pair(item.id, "", item.text, item.lang, TranscriptBubblePlacement.CENTER)
            i++
        }
    }
    // Determine alignment by first detected lang
    val firstLang = entries.filterIsInstance<TranscriptEntry.Pair>().firstOrNull { it.lang.isNotBlank() }?.lang.orEmpty()
    if (firstLang.isNotBlank()) {
        for (idx in entries.indices) {
            val e = entries[idx]
            if (e is TranscriptEntry.Pair) {
                val placement = when {
                    e.lang.isBlank() -> TranscriptBubblePlacement.CENTER
                    e.lang == firstLang -> TranscriptBubblePlacement.LEFT
                    else -> TranscriptBubblePlacement.RIGHT
                }
                entries[idx] = e.copy(placement = placement)
            }
        }
    }
    return entries
}

@Composable
internal fun TranscriptCard(
    transcripts: List<TranslationGummyTranscriptItem>,
    emptyLabel: String,
    inputChip: String,
    outputChip: String,
    accent: Color,
    listState: androidx.compose.foundation.lazy.LazyListState,
    modifier: Modifier = Modifier,
) {
    val surfaceLow = MaterialTheme.colorScheme.surfaceContainerLow
    Card(
        shape = MaterialTheme.shapes.large,
        colors = CardDefaults.cardColors(
            containerColor = lerp(surfaceLow, accent, 0.03f),
        ),
        modifier = modifier,
    ) {
        if (transcripts.isEmpty()) {
            Box(
                modifier = Modifier.fillMaxSize().padding(24.dp),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    emptyLabel,
                    style = MaterialTheme.typography.bodyMedium,
                    fontStyle = FontStyle.Italic,
                    color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.6f),
                )
            }
        } else {
            val entries = remember(transcripts) { groupEntries(transcripts) }
            LazyColumn(
                state = listState,
                verticalArrangement = Arrangement.spacedBy(8.dp),
                contentPadding = PaddingValues(12.dp),
                modifier = Modifier.fillMaxSize(),
            ) {
                items(entries.size, key = { idx ->
                    when (val e = entries[idx]) {
                        is TranscriptEntry.Pair -> e.id
                        is TranscriptEntry.Sep -> -e.id
                    }
                }) { idx ->
                    when (val entry = entries[idx]) {
                        is TranscriptEntry.Sep -> DashedSeparator(entry.time)
                        is TranscriptEntry.Pair -> ChatBubble(entry, accent)
                    }
                }
            }
        }
    }
}

@Composable
internal fun ChatBubble(pair: TranscriptEntry.Pair, accent: Color) {
    val surfaceLow = MaterialTheme.colorScheme.surfaceContainerLow
    val backgroundColor by animateColorAsState(
        targetValue = when (pair.placement) {
            TranscriptBubblePlacement.CENTER -> lerp(surfaceLow, accent, 0.03f)
            TranscriptBubblePlacement.LEFT -> lerp(surfaceLow, accent, 0.06f)
            TranscriptBubblePlacement.RIGHT -> lerp(surfaceLow, accent, 0.14f)
        },
        label = "bubbleBackgroundColor",
    )
    val outputColor by animateColorAsState(
        targetValue = when (pair.placement) {
            TranscriptBubblePlacement.CENTER -> MaterialTheme.colorScheme.onSurfaceVariant
            TranscriptBubblePlacement.LEFT -> MaterialTheme.colorScheme.onSurface
            TranscriptBubblePlacement.RIGHT -> accent
        },
        label = "bubbleOutputColor",
    )
    val bottomStartRadius by animateDpAsState(
        targetValue = when (pair.placement) {
            TranscriptBubblePlacement.LEFT -> 6.dp
            else -> 18.dp
        },
        label = "bubbleBottomStartRadius",
    )
    val bottomEndRadius by animateDpAsState(
        targetValue = when (pair.placement) {
            TranscriptBubblePlacement.RIGHT -> 6.dp
            else -> 18.dp
        },
        label = "bubbleBottomEndRadius",
    )

    val translationAnim = remember(pair.id) { Animatable(0f) }
    val alpha = remember(pair.id) { Animatable(0f) }
    val scale = remember(pair.id) { Animatable(0.96f) }
    var shown by remember(pair.id) { mutableStateOf(false) }

    BoxWithConstraints(
        modifier = Modifier.fillMaxWidth(),
        contentAlignment = Alignment.Center,
    ) {
        val containerWidthPx = with(LocalDensity.current) { maxWidth.toPx() }
        var bubbleWidthPx by remember(pair.id) { mutableFloatStateOf(0f) }
        val travelPx = (containerWidthPx - bubbleWidthPx).coerceAtLeast(0f)
        val targetTranslationPx = when {
            containerWidthPx <= 0f || bubbleWidthPx <= 0f -> 0f
            pair.placement == TranscriptBubblePlacement.CENTER -> 0f
            pair.placement == TranscriptBubblePlacement.LEFT -> -travelPx / 2f
            else -> travelPx / 2f
        }

        LaunchedEffect(pair.id) {
            translationAnim.snapTo(0f)
            alpha.snapTo(0f)
            scale.snapTo(0.96f)
            alpha.animateTo(1f, tween(140))
            scale.animateTo(1f, tween(140))
            shown = true
        }

        LaunchedEffect(pair.id, pair.placement, targetTranslationPx) {
            if (!shown && targetTranslationPx != 0f) {
                kotlinx.coroutines.delay(70)
            }
            if (targetTranslationPx == 0f) {
                translationAnim.animateTo(0f, tween(180))
            } else {
                translationAnim.animateTo(targetTranslationPx, tween(320))
            }
        }

        Card(
            modifier = Modifier
                .widthIn(max = 280.dp)
                .onGloballyPositioned { bubbleWidthPx = it.size.width.toFloat() }
                .graphicsLayer {
                    this.translationX = translationAnim.value
                    this.alpha = alpha.value
                    this.scaleX = scale.value
                    this.scaleY = scale.value
                },
            shape = RoundedCornerShape(18.dp, 18.dp, bottomEndRadius, bottomStartRadius),
            colors = CardDefaults.cardColors(
                containerColor = backgroundColor,
            ),
        ) {
            Column(modifier = Modifier.padding(10.dp, 8.dp)) {
                if (pair.input.isNotBlank()) {
                    Text(
                        pair.input,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                if (pair.output.isNotBlank()) {
                    Text(
                        pair.output,
                        style = MaterialTheme.typography.bodyLarge,
                        fontWeight = FontWeight.SemiBold,
                        color = outputColor,
                    )
                }
            }
        }
    }
}

@Composable
internal fun DashedSeparator(time: String) {
    val color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f)
    Row(
        modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        DashedLine(modifier = Modifier.weight(1f), color = color)
        Text(
            time,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.5f),
        )
        DashedLine(modifier = Modifier.weight(1f), color = color)
    }
}

@Composable
internal fun DashedLine(modifier: Modifier, color: Color) {
    Canvas(modifier = modifier.height(1.dp)) {
        drawLine(
            color = color,
            start = Offset(0f, size.height / 2),
            end = Offset(size.width, size.height / 2),
            strokeWidth = 1.dp.toPx(),
            pathEffect = PathEffect.dashPathEffect(floatArrayOf(4.dp.toPx(), 4.dp.toPx())),
        )
    }
}

// ── Status Dot ──

