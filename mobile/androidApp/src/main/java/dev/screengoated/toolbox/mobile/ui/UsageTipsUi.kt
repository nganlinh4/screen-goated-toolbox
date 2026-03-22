@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.Lightbulb
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import kotlin.random.Random
import kotlinx.coroutines.delay

internal const val USAGE_TIP_FADE_DURATION_MS: Long = 500L

internal fun usageTipDisplayDurationMillis(text: String): Long = 2000L + text.length * 60L

internal fun selectNextUsageTipIndex(
    currentIndex: Int,
    tipCount: Int,
    random: Random,
): Int {
    if (tipCount <= 1) {
        return if (tipCount == 1) 0 else -1
    }
    val next = random.nextInt(tipCount)
    return if (next == currentIndex) {
        (next + 1) % tipCount
    } else {
        next
    }
}

@Composable
internal fun UsageTipsCard(
    locale: MobileLocaleText,
    modifier: Modifier = Modifier,
) {
    var showDialog by rememberSaveable { mutableStateOf(false) }
    val tips = locale.usageTipsList
    val preview = rememberUsageTipsPreview(
        tips = tips,
        paused = showDialog,
    )
    val previewText = if (preview.currentIndex in tips.indices) {
        tips[preview.currentIndex]
    } else {
        ""
    }

    Card(
        modifier = modifier
            .fillMaxWidth()
            .clickable(enabled = tips.isNotEmpty()) { showDialog = true },
        shape = MaterialTheme.shapes.extraLarge,
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
        ),
    ) {
        Column(
            modifier = Modifier.padding(ShellSpacing.innerPad),
            verticalArrangement = Arrangement.spacedBy(ShellSpacing.itemGap),
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Icon(
                    imageVector = Icons.Rounded.Lightbulb,
                    contentDescription = null,
                    modifier = Modifier.size(24.dp),
                    tint = MaterialTheme.colorScheme.primary,
                )
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = locale.usageTipsTitle,
                        style = MaterialTheme.typography.titleSmall,
                        fontWeight = FontWeight.Bold,
                    )
                    Text(
                        text = locale.usageTipsClickHint,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }

            if (tips.isEmpty()) {
                Text(
                    text = locale.usageTipsClickHint,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            } else {
                Text(
                    text = rememberUsageTipAnnotatedString(
                        text = previewText,
                        regularColor = MaterialTheme.colorScheme.onSurface,
                        boldColor = MaterialTheme.colorScheme.primary,
                    ),
                    style = MaterialTheme.typography.bodyMedium,
                    maxLines = 3,
                    modifier = Modifier.graphicsLayer(alpha = preview.alpha),
                )
            }
        }
    }

    if (showDialog) {
        UsageTipsDialog(
            locale = locale,
            onDismiss = { showDialog = false },
        )
    }
}

@Composable
private fun UsageTipsDialog(
    locale: MobileLocaleText,
    onDismiss: () -> Unit,
) {
    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false),
    ) {
        Card(
            modifier = Modifier
                .fillMaxWidth(0.94f)
                .widthIn(max = 560.dp)
                .padding(16.dp),
            shape = MaterialTheme.shapes.extraLarge,
            colors = CardDefaults.cardColors(
                containerColor = MaterialTheme.colorScheme.surface,
            ),
        ) {
            BoxWithConstraints(
                modifier = Modifier
                    .fillMaxWidth()
                    .fillMaxHeight(0.76f)
                    .heightIn(max = 620.dp)
                    .padding(start = 20.dp, end = 12.dp, top = 12.dp, bottom = 16.dp),
            ) {
                Column(modifier = Modifier.fillMaxWidth()) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(
                            text = locale.usageTipsTitle,
                            style = MaterialTheme.typography.titleLarge,
                            fontWeight = FontWeight.SemiBold,
                        )
                        Spacer(Modifier.weight(1f))
                        IconButton(onClick = onDismiss) {
                            Icon(Icons.Rounded.Close, contentDescription = null)
                        }
                    }

                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .weight(1f)
                            .verticalScroll(rememberScrollState())
                            .padding(end = 8.dp),
                        verticalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        locale.usageTipsList.forEachIndexed { index, tip ->
                            UsageTipListRow(
                                tipNumber = index + 1,
                                text = tip,
                            )
                            if (index < locale.usageTipsList.lastIndex) {
                                HorizontalDivider()
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun UsageTipListRow(
    tipNumber: Int,
    text: String,
) {
    val regularColor = MaterialTheme.colorScheme.onSurfaceVariant
    val boldColor = MaterialTheme.colorScheme.primary
    val prefix = "$tipNumber. "
    Row(
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text(
            text = prefix,
            style = MaterialTheme.typography.bodyMedium,
            color = regularColor,
            fontWeight = FontWeight.Medium,
        )
        Text(
            text = rememberUsageTipAnnotatedString(
                text = text,
                regularColor = regularColor,
                boldColor = boldColor,
            ),
            style = MaterialTheme.typography.bodyMedium,
            modifier = Modifier.weight(1f),
        )
    }
}

private data class UsageTipsPreviewState(
    val currentIndex: Int,
    val alpha: Float,
)

@Composable
private fun rememberUsageTipsPreview(
    tips: List<String>,
    paused: Boolean,
): UsageTipsPreviewState {
    var currentIndex by remember(tips) {
        mutableIntStateOf(if (tips.isNotEmpty()) 0 else -1)
    }
    val alpha = remember { Animatable(0f) }
    val random = remember { Random(System.currentTimeMillis()) }

    LaunchedEffect(tips, paused) {
        if (tips.isEmpty()) {
            currentIndex = -1
            alpha.snapTo(0f)
            return@LaunchedEffect
        }
        if (currentIndex !in tips.indices) {
            currentIndex = 0
        }
        if (paused) {
            alpha.snapTo(1f)
            return@LaunchedEffect
        }

        while (true) {
            if (alpha.value < 1f) {
                alpha.animateTo(
                    targetValue = 1f,
                    animationSpec = tween(
                        durationMillis = USAGE_TIP_FADE_DURATION_MS.toInt(),
                        easing = LinearEasing,
                    ),
                )
            }
            delay(usageTipDisplayDurationMillis(tips[currentIndex]))
            alpha.animateTo(
                targetValue = 0f,
                animationSpec = tween(
                    durationMillis = USAGE_TIP_FADE_DURATION_MS.toInt(),
                    easing = LinearEasing,
                ),
            )
            currentIndex = selectNextUsageTipIndex(
                currentIndex = currentIndex,
                tipCount = tips.size,
                random = random,
            )
        }
    }

    return UsageTipsPreviewState(
        currentIndex = currentIndex,
        alpha = alpha.value,
    )
}

@Composable
private fun rememberUsageTipAnnotatedString(
    text: String,
    regularColor: androidx.compose.ui.graphics.Color,
    boldColor: androidx.compose.ui.graphics.Color,
): AnnotatedString = remember(text, regularColor, boldColor) {
    buildAnnotatedString {
        var start = 0
        var isBold = false
        while (start < text.length) {
            val markerIndex = text.indexOf("**", startIndex = start)
            if (markerIndex < 0) {
                appendSegment(
                    segment = text.substring(start),
                    isBold = isBold,
                    regularColor = regularColor,
                    boldColor = boldColor,
                )
                break
            }
            if (markerIndex > start) {
                appendSegment(
                    segment = text.substring(start, markerIndex),
                    isBold = isBold,
                    regularColor = regularColor,
                    boldColor = boldColor,
                )
            }
            isBold = !isBold
            start = markerIndex + 2
        }
    }
}

private fun AnnotatedString.Builder.appendSegment(
    segment: String,
    isBold: Boolean,
    regularColor: androidx.compose.ui.graphics.Color,
    boldColor: androidx.compose.ui.graphics.Color,
) {
    if (segment.isEmpty()) {
        return
    }
    pushStyle(
        SpanStyle(
            color = if (isBold) boldColor else regularColor,
            fontWeight = if (isBold) FontWeight.SemiBold else FontWeight.Normal,
        ),
    )
    append(segment)
    pop()
}
