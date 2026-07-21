@file:OptIn(androidx.compose.material3.ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.creation

import android.graphics.BitmapFactory
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
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.ui.UtilityStatusChip
import dev.screengoated.toolbox.mobile.ui.i18n.CreationCommonLocale
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

@Composable
internal fun CreationQueueStrip(
    items: List<CreationNativeItem>,
    selectedId: String?,
    common: CreationCommonLocale,
    accent: Color,
    onSelect: (String) -> Unit,
    onRemove: (String) -> Unit,
    onAdd: () -> Unit,
) {
    LazyRow(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        item(key = "add") {
            AddImageTile(common = common, accent = accent, onClick = onAdd)
        }
        items(items, key = { it.id }) { item ->
            QueueItemTile(
                item = item,
                selected = item.id == selectedId,
                accent = accent,
                common = common,
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
    common: CreationCommonLocale,
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
                common.addImages,
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
            CreationImageThumbnail(
                path = item.sourcePath,
                modifier = Modifier
                    .size(50.dp)
                    .clip(MaterialTheme.shapes.small),
            )
            Spacer(Modifier.width(8.dp))
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    item.sourceName,
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
internal fun CreationImageThumbnail(path: String, modifier: Modifier = Modifier) {
    val bitmap by produceState<androidx.compose.ui.graphics.ImageBitmap?>(null, path) {
        value = withContext(Dispatchers.IO) {
            runCatching {
                val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
                BitmapFactory.decodeFile(path, bounds)
                val largest = maxOf(bounds.outWidth, bounds.outHeight).coerceAtLeast(1)
                val sample = Integer.highestOneBit((largest / 512).coerceAtLeast(1))
                BitmapFactory.decodeFile(
                    path,
                    BitmapFactory.Options().apply { inSampleSize = sample },
                )?.asImageBitmap()
            }.getOrNull()
        }
    }
    Box(
        modifier = modifier.background(MaterialTheme.colorScheme.surfaceContainerHighest),
        contentAlignment = Alignment.Center,
    ) {
        if (bitmap != null) {
            Image(
                bitmap = requireNotNull(bitmap),
                contentDescription = null,
                modifier = Modifier.fillMaxSize(),
                contentScale = ContentScale.Crop,
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
    content: @Composable () -> Unit,
) {
    Surface(
        modifier = modifier
            .fillMaxWidth()
            .aspectRatio(1.12f),
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
    val progress = estimatedProgress(status)
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.scrim.copy(alpha = 0.42f)),
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
            Text(
                "${(progress * 100).toInt()}%",
                style = MaterialTheme.typography.labelMedium,
                color = Color.White.copy(alpha = 0.84f),
            )
        }
    }
}

@Composable
internal fun CreationReadinessChip(status: String, common: CreationCommonLocale, accent: Color) {
    UtilityStatusChip(
        text = if (status == "ready" || status == "partial") common.ready else common.preparing,
        accent = if (status == "ready" || status == "partial") accent
        else MaterialTheme.colorScheme.tertiary,
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
    val observed = status.progressRatio?.toFloat()?.coerceIn(0f, 0.96f) ?: 0f
    val elapsed = status.elapsedMs?.coerceAtLeast(0L) ?: 0L
    val estimate = status.estimatedTotalMs?.coerceAtLeast(10_000L) ?: 240_000L
    val curve = (0.9 * (1.0 - kotlin.math.exp(-3.0 * elapsed / estimate.toDouble())))
        .toFloat()
        .coerceAtMost(0.94f)
    return maxOf(0.04f, observed, curve)
}
