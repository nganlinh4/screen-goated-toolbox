@file:OptIn(ExperimentalMaterial3ExpressiveApi::class, ExperimentalMaterial3Api::class, ExperimentalTextApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.ContentCopy
import androidx.compose.material.icons.rounded.Delete
import androidx.compose.material.icons.rounded.Star
import androidx.compose.material.icons.rounded.StarOutline
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.carousel.HorizontalUncontainedCarousel
import androidx.compose.material3.carousel.rememberCarouselState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.ui.theme.sgtColors
import kotlinx.coroutines.launch

/** Font family at a specific wdth axis value. */
private fun flexFontFamily(wdth: Int): FontFamily {
    return if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.O) {
        FontFamily(
            androidx.compose.ui.text.font.Font(
                resId = dev.screengoated.toolbox.mobile.R.font.google_sans_flex,
                weight = FontWeight.Medium,
                variationSettings = androidx.compose.ui.text.font.FontVariation.Settings(
                    androidx.compose.ui.text.font.FontVariation.weight(FontWeight.Medium.weight),
                    androidx.compose.ui.text.font.FontVariation.Setting("ROND", 100f),
                    androidx.compose.ui.text.font.FontVariation.Setting("wdth", wdth.toFloat()),
                ),
            ),
        )
    } else {
        FontFamily.Default
    }
}

/** Condense steps: 100 → 90 → 80 → 70 → 62 (Google Sans Flex minimum). */
internal val condensedFontSteps: List<Pair<Int, FontFamily>> by lazy {
    listOf(100, 90, 80, 70, 62).map { wdth -> wdth to flexFontFamily(wdth) }
}

/** Stretch steps: 100 → 110 → 120 → 125 (Google Sans Flex maximum). */
private val stretchedFontSteps: List<Pair<Int, FontFamily>> by lazy {
    listOf(100, 110, 120, 125).map { wdth -> wdth to flexFontFamily(wdth) }
}

private fun fontFamilyForIndex(idx: Int): FontFamily = when {
    idx > 0 -> stretchedFontSteps.getOrElse(idx) { stretchedFontSteps.last() }.second
    idx < 0 -> condensedFontSteps.getOrElse(-idx) { condensedFontSteps.last() }.second
    else -> condensedFontSteps[0].second
}

// Cache settled font width index per text string across recompositions/page revisits
private val flexWidthCache = HashMap<String, Int>(64)

/** Single-line text that independently auto-adjusts wdth: stretches short text, condenses long. */
@Composable
private fun AutoFlexLine(
    text: String,
    color: Color,
    modifier: Modifier = Modifier,
) {
    val style = MaterialTheme.typography.labelLarge
    val cached = flexWidthCache[text]
    var stretchIdx by remember(text) { mutableIntStateOf(cached ?: 0) }
    var tryStretch by remember(text) { mutableIntStateOf(if (cached != null) 0 else 1) }
    val fontFamily = remember(stretchIdx) { fontFamilyForIndex(stretchIdx) }

    Text(
        text = text,
        style = style,
        fontFamily = fontFamily,
        fontWeight = FontWeight.Medium,
        color = color,
        maxLines = 1,
        textAlign = androidx.compose.ui.text.style.TextAlign.Start,
        modifier = modifier,
        onTextLayout = { result ->
            if (result.hasVisualOverflow) {
                if (tryStretch > 0) {
                    tryStretch = 0
                    stretchIdx = 1
                }
                if (-stretchIdx < condensedFontSteps.lastIndex) {
                    stretchIdx -= 1
                }
            } else if (tryStretch > 0 && tryStretch <= stretchedFontSteps.lastIndex) {
                stretchIdx = tryStretch
                tryStretch++
            } else {
                flexWidthCache[text] = stretchIdx
            }
        },
    )
}

/** Two independently flex-width lines from a balanced name split. */
@Composable
private fun AutoFlexTwoLines(
    text: String,
    color: Color,
    modifier: Modifier = Modifier,
) {
    val parts = text.split("\n", limit = 2)
    val line1 = parts[0]
    val line2 = if (parts.size > 1) parts[1].trim() else ""
    Column(
        modifier = modifier,
        horizontalAlignment = Alignment.Start,
    ) {
        AutoFlexLine(text = line1, color = color, modifier = Modifier.fillMaxWidth())
        if (line2.isNotEmpty()) {
            AutoFlexLine(text = line2, color = color, modifier = Modifier.fillMaxWidth())
        }
    }
}

@Composable
internal fun ToolCategoryRow(
    label: String,
    accentColor: Color,
    presets: List<ToolPresetItem>,
    lang: String,
    onPresetClick: (String) -> Unit = {},
    onPagerSwipeLockChanged: (Boolean) -> Unit = {},
    toolbarMode: ToolbarMode = ToolbarMode.NONE,
    favoritePresetIds: Set<String> = emptySet(),
    onFavoriteToggle: (String) -> Unit = {},
    onDuplicate: (String) -> Unit = {},
    onDelete: (String) -> Unit = {},
) {
    val trailingClearance = 12.dp
    // Track presets being deleted for fade-out animation
    var deletingIds by remember { mutableStateOf(emptySet<String>()) }

    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        // Category label
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier.padding(horizontal = 4.dp),
        ) {
            Box(
                modifier = Modifier
                    .size(width = 22.dp, height = 8.dp)
                    .background(accentColor, CircleShape),
            )
            Spacer(Modifier.width(6.dp))
            Text(
                text = label,
                style = MaterialTheme.typography.labelLarge,
                fontWeight = FontWeight.SemiBold,
                color = accentColor,
            )
        }

        val bgColor = MaterialTheme.colorScheme.background
        val fadePx = with(androidx.compose.ui.platform.LocalDensity.current) { 24.dp.toPx() }
        val carouselState = rememberCarouselState { presets.size }
        // Auto-scroll to end when a new preset is added
        val prevCount = remember { mutableIntStateOf(presets.size) }
        val scope = rememberCoroutineScope()
        LaunchedEffect(presets.size) {
            if (presets.size > prevCount.intValue) {
                scope.launch {
                    try { carouselState.animateScrollToItem(presets.lastIndex) } catch (_: Exception) {}
                }
            }
            prevCount.intValue = presets.size
        }
        val scrollFraction by remember {
            derivedStateOf {
                val max = (presets.size - 1).coerceAtLeast(1)
                carouselState.currentItem.toFloat() / max.toFloat()
            }
        }
        HorizontalUncontainedCarousel(
            state = carouselState,
            itemWidth = 150.dp,
            itemSpacing = 8.dp,
            contentPadding = PaddingValues(start = 4.dp, end = trailingClearance),
            modifier = Modifier
                .fillMaxWidth()
                .lockPagerForCarouselDrag(
                    canScrollBackward = { carouselState.canScrollBackward },
                    canScrollForward = { carouselState.canScrollForward },
                    onPagerSwipeLockChanged = onPagerSwipeLockChanged,
                )
                .drawWithContent {
                    drawContent()
                    val rightAlpha = (1f - scrollFraction).coerceIn(0f, 1f)
                    if (rightAlpha > 0.01f) {
                        drawRect(
                            brush = Brush.horizontalGradient(
                                colors = listOf(Color.Transparent, bgColor.copy(alpha = rightAlpha)),
                                startX = size.width - fadePx,
                                endX = size.width,
                            ),
                        )
                    }
                    val leftAlpha = scrollFraction.coerceIn(0f, 1f)
                    if (leftAlpha > 0.01f) {
                        drawRect(
                            brush = Brush.horizontalGradient(
                                colors = listOf(bgColor.copy(alpha = leftAlpha), Color.Transparent),
                                startX = 0f,
                                endX = fadePx,
                            ),
                        )
                    }
                },
        ) { index ->
            val preset = presets[index]
            val presetId = if (preset.isFullId) preset.id else "preset_${preset.id}"
            val isActionMode = toolbarMode != ToolbarMode.NONE
            val isFavorite = presetId in favoritePresetIds
            val isDeleting = presetId in deletingIds

            // Animate fade-out + shrink when deleting
            val deleteAlpha by animateFloatAsState(
                targetValue = if (isDeleting) 0f else 1f,
                animationSpec = spring(stiffness = Spring.StiffnessMediumLow),
                label = "del-alpha-$index",
                finishedListener = { value ->
                    if (value == 0f) {
                        deletingIds = deletingIds - presetId
                        onDelete(presetId)
                    }
                },
            )
            val deleteScale by animateFloatAsState(
                targetValue = if (isDeleting) 0.6f else 1f,
                animationSpec = spring(
                    dampingRatio = Spring.DampingRatioMediumBouncy,
                    stiffness = Spring.StiffnessMediumLow,
                ),
                label = "del-scale-$index",
            )
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .graphicsLayer {
                        alpha = deleteAlpha
                        scaleX = deleteScale
                        scaleY = deleteScale
                    }
                    .maskClip(MaterialTheme.shapes.large)
                    .clickable(enabled = !isActionMode && !isDeleting) { onPresetClick(presetId) },
            ) {
                Card(
                    modifier = Modifier.fillMaxSize(),
                    shape = MaterialTheme.shapes.large,
                    colors = CardDefaults.cardColors(
                        containerColor = MaterialTheme.colorScheme.surfaceContainerLow.copy(alpha = 0.96f),
                    ),
                ) {
                    Row(
                        modifier = Modifier
                            .fillMaxSize()
                            .background(
                                Brush.verticalGradient(
                                    listOf(
                                        accentColor.copy(alpha = 0.2f),
                                        accentColor.copy(alpha = 0.08f),
                                        MaterialTheme.colorScheme.surfaceContainerLow,
                                    ),
                                ),
                            )
                            .padding(horizontal = 10.dp, vertical = 10.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Box(
                            modifier = Modifier
                                .size(42.dp)
                                .background(
                                    color = accentColor.copy(alpha = 0.2f),
                                    shape = MaterialTheme.shapes.large,
                                ),
                            contentAlignment = Alignment.Center,
                        ) {
                            Icon(
                                preset.icon,
                                contentDescription = null,
                                tint = accentColor,
                                modifier = Modifier.size(24.dp),
                            )
                        }
                        Spacer(Modifier.width(10.dp))
                        AutoFlexTwoLines(
                            text = preset.balancedName(lang),
                            color = MaterialTheme.colorScheme.onSurface,
                            modifier = Modifier.weight(1f),
                        )
                    }
                }
                if (toolbarMode == ToolbarMode.FAVORITE) {
                    IconButton(
                        onClick = { onFavoriteToggle(presetId) },
                        modifier = Modifier
                            .align(Alignment.CenterEnd)
                            .padding(end = 10.dp)
                            .size(40.dp)
                            .background(
                                color = MaterialTheme.colorScheme.surfaceContainerHighest.copy(alpha = 0.92f),
                                shape = CircleShape,
                            ),
                    ) {
                        Icon(
                            imageVector = if (isFavorite) Icons.Rounded.Star else Icons.Rounded.StarOutline,
                            contentDescription = null,
                            tint = if (isFavorite) MaterialTheme.sgtColors.favoriteStar else MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
                if (toolbarMode == ToolbarMode.DUPLICATE) {
                    IconButton(
                        onClick = { onDuplicate(presetId) },
                        modifier = Modifier
                            .align(Alignment.CenterEnd)
                            .padding(end = 10.dp)
                            .size(40.dp)
                            .background(
                                color = MaterialTheme.colorScheme.surfaceContainerHighest.copy(alpha = 0.92f),
                                shape = CircleShape,
                            ),
                    ) {
                        Icon(
                            imageVector = Icons.Rounded.ContentCopy,
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.primary,
                        )
                    }
                }
                if (toolbarMode == ToolbarMode.DELETE && !isDeleting) {
                    IconButton(
                        onClick = { deletingIds = deletingIds + presetId },
                        modifier = Modifier
                            .align(Alignment.CenterEnd)
                            .padding(end = 10.dp)
                            .size(40.dp)
                            .background(
                                color = MaterialTheme.colorScheme.errorContainer.copy(alpha = 0.88f),
                                shape = CircleShape,
                            ),
                    ) {
                        Icon(
                            imageVector = Icons.Rounded.Delete,
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.error,
                        )
                    }
                }
            } // Box
        }
    }
}

internal fun Modifier.lockPagerForCarouselDrag(
    canScrollBackward: () -> Boolean,
    canScrollForward: () -> Boolean,
    onPagerSwipeLockChanged: (Boolean) -> Unit,
): Modifier = pointerInput(onPagerSwipeLockChanged) {
    awaitEachGesture {
        awaitFirstDown(requireUnconsumed = false)
        onPagerSwipeLockChanged(
            shouldLockPagerForCarouselTouch(
                canScrollBackward = canScrollBackward(),
                canScrollForward = canScrollForward(),
            ),
        )
        try {
            while (true) {
                val event = awaitPointerEvent()
                val change = event.changes.firstOrNull() ?: break
                if (!change.pressed) break
                val deltaX = change.positionChange().x
                if (deltaX != 0f) {
                    onPagerSwipeLockChanged(
                        shouldLockPagerForCarouselTouch(
                            canScrollBackward = canScrollBackward(),
                            canScrollForward = canScrollForward(),
                        ),
                    )
                }
            }
        } finally {
            onPagerSwipeLockChanged(false)
        }
    }
}

internal fun shouldLockPagerForCarouselTouch(
    canScrollBackward: Boolean,
    canScrollForward: Boolean,
): Boolean = canScrollBackward || canScrollForward
