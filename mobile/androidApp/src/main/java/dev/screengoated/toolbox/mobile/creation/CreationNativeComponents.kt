@file:OptIn(androidx.compose.material3.ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.creation

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearWavyProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.ui.UtilityStatusChip
import dev.screengoated.toolbox.mobile.ui.i18n.CreationCommonLocale
import kotlinx.coroutines.awaitCancellation

@Composable
internal fun CreationQueueStrip(
    items: List<CreationNativeItem>,
    selectedId: String?,
    common: CreationCommonLocale,
    accent: Color,
    onSelect: (String) -> Unit,
    onRemove: (String) -> Unit,
    onAdd: () -> Unit,
    addLabel: String,
    itemLabel: (CreationNativeItem) -> String,
    showArtworkPreviews: Boolean,
) {
    LazyRow(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        item(key = "add") {
            AddImageTile(label = addLabel, accent = accent, onClick = onAdd)
        }
        items(items.sortedByDescending(CreationNativeItem::createdAtMs), key = { it.id }) { item ->
            QueueItemTile(
                item = item,
                selected = item.id == selectedId,
                accent = accent,
                common = common,
                displayName = itemLabel(item),
                showArtworkPreview = showArtworkPreviews,
                onClick = { onSelect(item.id) },
                onRemove = { onRemove(item.id) },
            )
        }
    }
}

@Composable
internal fun CreationHistoryStrip(
    entries: List<CreationHistoryEntry>,
    selectedId: String?,
    common: CreationCommonLocale,
    accent: Color,
    onSelect: (String) -> Unit,
) {
    if (entries.isEmpty()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(vertical = 18.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.Center,
        ) {
            Icon(
                painterResource(R.drawable.ms_history),
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Spacer(Modifier.width(8.dp))
            Text(
                common.noResults,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        return
    }
    LazyRow(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        items(entries, key = { it.id }) { entry ->
            Card(
                onClick = { onSelect(entry.id) },
                modifier = Modifier
                    .width(142.dp)
                    .height(66.dp)
                    .then(
                        if (entry.id == selectedId) {
                            Modifier.border(2.dp, accent, MaterialTheme.shapes.medium)
                        } else Modifier
                    ),
                shape = MaterialTheme.shapes.medium,
                colors = CardDefaults.cardColors(
                    containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
                ),
            ) {
                Row(
                    modifier = Modifier
                        .fillMaxSize()
                        .padding(10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(9.dp),
                ) {
                    Surface(
                        modifier = Modifier.size(34.dp),
                        shape = CircleShape,
                        color = accent.copy(alpha = 0.14f),
                    ) {
                        Box(contentAlignment = Alignment.Center) {
                            Icon(
                                painterResource(R.drawable.ms_check),
                                contentDescription = null,
                                tint = accent,
                                modifier = Modifier.size(18.dp),
                            )
                        }
                    }
                    Text(
                        entry.outputName,
                        style = MaterialTheme.typography.labelMedium,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        }
    }
}

@Composable
private fun AddImageTile(
    label: String,
    accent: Color,
    onClick: () -> Unit,
) {
    Surface(
        modifier = Modifier
            .size(width = 104.dp, height = 66.dp)
            .clickable(onClick = onClick),
        shape = MaterialTheme.shapes.medium,
        color = accent.copy(alpha = 0.12f),
    ) {
        Column(
            modifier = Modifier.padding(9.dp),
            verticalArrangement = Arrangement.Center,
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Icon(
                painterResource(R.drawable.ms_add),
                contentDescription = null,
                tint = accent,
                modifier = Modifier.size(20.dp),
            )
            Text(
                label,
                style = MaterialTheme.typography.labelSmall,
                color = accent,
                maxLines = 1,
            )
        }
    }
}

@Composable
private fun QueueItemTile(
    item: CreationNativeItem,
    selected: Boolean,
    accent: Color,
    common: CreationCommonLocale,
    displayName: String,
    showArtworkPreview: Boolean,
    onClick: () -> Unit,
    onRemove: () -> Unit,
) {
    Card(
        onClick = onClick,
        modifier = Modifier
            .width(168.dp)
            .height(66.dp)
            .then(
                if (selected) Modifier.border(2.dp, accent, MaterialTheme.shapes.medium)
                else Modifier
            ),
        shape = MaterialTheme.shapes.medium,
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
        ),
    ) {
        Row(
            modifier = Modifier
                .fillMaxSize()
                .padding(7.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (showArtworkPreview && item.sourcePath.isNotBlank()) {
                CreationImageThumbnail(
                    path = item.sourcePath,
                    maximumEdgePixels = 128,
                    modifier = Modifier
                        .size(50.dp)
                        .clip(MaterialTheme.shapes.small),
                )
            } else {
                Box(
                    modifier = Modifier
                        .size(50.dp)
                        .clip(MaterialTheme.shapes.small)
                        .background(MaterialTheme.colorScheme.surfaceContainerHighest),
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(
                        painterResource(
                            if (item.sourcePath.isBlank()) {
                                R.drawable.ms_auto_awesome
                            } else {
                                R.drawable.ms_image
                            },
                        ),
                        contentDescription = null,
                        tint = accent,
                    )
                }
            }
            Spacer(Modifier.width(8.dp))
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    displayName,
                    style = MaterialTheme.typography.labelMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    nativeStageLabel(item.stage, common),
                    style = MaterialTheme.typography.labelSmall,
                    color = stageColor(item.stage, accent),
                    maxLines = 1,
                )
            }
            if (item.stage != CreationNativeStage.RUNNING) {
                IconButton(onClick = onRemove, modifier = Modifier.size(28.dp)) {
                    Icon(
                        painterResource(R.drawable.ms_close),
                        contentDescription = common.dismiss,
                        modifier = Modifier.size(16.dp),
                    )
                }
            }
        }
    }
}

@Composable
internal fun CreationImageThumbnail(
    path: String,
    modifier: Modifier = Modifier,
    contentScale: ContentScale = ContentScale.Crop,
    maximumEdgePixels: Int = 1_600,
) {
    val context = LocalContext.current
    val bitmap by produceState<android.graphics.Bitmap?>(
        null,
        path,
        maximumEdgePixels,
    ) {
        val loaded = decodeCreationThumbnail(context, path, maximumEdgePixels)
        value = loaded
        try {
            awaitCancellation()
        } finally {
            loaded?.recycle()
        }
    }
    Box(
        modifier = modifier.background(Color.Transparent),
        contentAlignment = Alignment.Center,
    ) {
        if (bitmap != null) {
            Image(
                bitmap = requireNotNull(bitmap).asImageBitmap(),
                contentDescription = null,
                modifier = Modifier.fillMaxSize(),
                contentScale = contentScale,
            )
        } else {
            Icon(
                painterResource(R.drawable.ms_image),
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
internal fun CreationWorkbench(
    modifier: Modifier = Modifier,
    accent: Color,
    fillAvailable: Boolean = false,
    content: @Composable () -> Unit,
) {
    val workbenchModifier = if (fillAvailable) {
        modifier.fillMaxSize()
    } else {
        modifier.fillMaxWidth().aspectRatio(1.12f)
    }
    Surface(
        modifier = workbenchModifier,
        shape = MaterialTheme.shapes.large,
        color = MaterialTheme.colorScheme.surfaceContainerLow,
        tonalElevation = 1.dp,
    ) {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(accent.copy(alpha = 0.025f)),
            contentAlignment = Alignment.Center,
        ) {
            content()
        }
    }
}

@Composable
internal fun CreationEmptyWorkbench(
    common: CreationCommonLocale,
    accent: Color,
    onAdd: () -> Unit,
) {
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp),
        modifier = Modifier.clickable(onClick = onAdd).padding(28.dp),
    ) {
        Surface(shape = CircleShape, color = accent.copy(alpha = 0.14f)) {
            Box(Modifier.size(58.dp), contentAlignment = Alignment.Center) {
                Icon(
                    painterResource(R.drawable.ms_image),
                    contentDescription = null,
                    tint = accent,
                    modifier = Modifier.size(26.dp),
                )
            }
        }
        Text(common.noImages, style = MaterialTheme.typography.titleMedium)
        Text(
            common.chooseImages,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
internal fun CreationSourceWorkbench(item: CreationNativeItem) {
    CreationImageThumbnail(
        path = item.sourcePath,
        modifier = Modifier.fillMaxSize(),
    )
}

@Composable
internal fun CreationProgressOverlay(
    status: CreationJobStatus?,
    common: CreationCommonLocale,
    accent: Color,
) {
    val stage = status?.toNativeStage() ?: CreationNativeStage.QUEUED
    val candidate = estimatedProgress(status)
    val progressFloor = remember(status?.jobId) { mutableFloatStateOf(0.04f) }
    val progress = maxOf(progressFloor.floatValue, candidate)
    LaunchedEffect(progress) { progressFloor.floatValue = progress }
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.scrim.copy(alpha = 0.32f)),
        contentAlignment = Alignment.BottomCenter,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(20.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(
                nativeStageLabel(stage, common),
                style = MaterialTheme.typography.titleMedium,
                color = Color.White,
            )
            LinearWavyProgressIndicator(
                progress = { progress },
                modifier = Modifier.fillMaxWidth().height(6.dp),
                color = accent,
                trackColor = Color.White.copy(alpha = 0.25f),
            )
            Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Text(
                        "${(progress * 100).toInt()}%",
                        style = MaterialTheme.typography.labelMedium,
                        color = Color.White.copy(alpha = 0.84f),
                    )
                    Text(
                        estimatedProgressEta(status, common),
                        style = MaterialTheme.typography.labelMedium,
                        color = Color.White.copy(alpha = 0.84f),
                    )
            }
        }
    }
}

@Composable
internal fun CreationReadinessChip(status: String, common: CreationCommonLocale, accent: Color) {
    UtilityStatusChip(
        text = when (status) {
            "ready" -> common.ready
            "unavailable" -> common.failed
            else -> common.preparing
        },
        accent = when (status) {
            "ready" -> accent
            "unavailable" -> MaterialTheme.colorScheme.error
            else -> MaterialTheme.colorScheme.tertiary
        },
    )
}

internal fun nativeStageLabel(stage: CreationNativeStage, common: CreationCommonLocale): String =
    when (stage) {
        CreationNativeStage.DRAFT -> common.ready
        CreationNativeStage.QUEUED -> common.queued
        CreationNativeStage.RUNNING -> common.working
        CreationNativeStage.DONE -> common.done
        CreationNativeStage.FAILED -> common.failed
        CreationNativeStage.CANCELLED -> common.cancel
    }

private fun stageColor(stage: CreationNativeStage, accent: Color): Color = when (stage) {
    CreationNativeStage.FAILED -> Color(0xffba1a1a)
    CreationNativeStage.CANCELLED -> Color(0xff72777a)
    CreationNativeStage.DONE -> accent
    else -> accent
}

private fun estimatedProgress(status: CreationJobStatus?): Float {
    if (status == null) return 0.04f
    val observed = status.progressRatio?.toFloat()?.coerceIn(0f, 0.94f) ?: 0f
    val elapsed = status.elapsedMs?.coerceAtLeast(0L) ?: 0L
    val estimate = status.estimatedTotalMs?.coerceAtLeast(10_000L) ?: 240_000L
    val curve = (0.9 * (1.0 - kotlin.math.exp(-3.0 * elapsed / estimate.toDouble())))
        .toFloat()
        .coerceAtMost(0.94f)
    if (status.stage == "preparing") {
        val preparationCurve = (0.04f + curve * 0.16f).coerceAtMost(0.18f)
        return maxOf(0.04f, observed.coerceAtMost(0.18f), preparationCurve)
    }
    return maxOf(0.04f, observed, curve)
}

private fun estimatedProgressEta(
    status: CreationJobStatus?,
    common: CreationCommonLocale,
): String {
    val elapsed = status?.elapsedMs?.coerceAtLeast(0L) ?: 0L
    val estimate = status?.estimatedTotalMs?.coerceAtLeast(10_000L) ?: 240_000L
    if (elapsed >= estimate) return common.progress.takingLonger
    val remaining = estimate - elapsed
    if (remaining <= 15_000L) return common.progress.almostThere
    if (remaining < 60_000L) return common.progress.lessThanMinute
    val minutes = maxOf(1L, (remaining + 59_999L) / 60_000L)
    return common.progress.aboutMinutes.replace("{count}", minutes.toString())
}
