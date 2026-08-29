package dev.screengoated.toolbox.mobile.ui

import androidx.annotation.DrawableRes
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.preset.PresetModelDescriptor

internal const val MODEL_INTELLIGENCE_COLUMN_WIDTH_DP = 15
internal const val MODEL_PERFORMANCE_COLUMN_GAP_DP = 2
internal const val MODEL_LATENCY_COLUMN_WIDTH_DP = 32
internal const val MODEL_PERFORMANCE_PREFIX_WIDTH_DP =
    MODEL_INTELLIGENCE_COLUMN_WIDTH_DP +
        MODEL_PERFORMANCE_COLUMN_GAP_DP +
        MODEL_LATENCY_COLUMN_WIDTH_DP

@Composable
internal fun ModelPerformancePrefix(
    model: PresetModelDescriptor?,
    modifier: Modifier = Modifier,
    latencyOverrideMs: Int? = null,
) {
    Row(
        modifier = modifier.width(MODEL_PERFORMANCE_PREFIX_WIDTH_DP.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier = Modifier.width(MODEL_INTELLIGENCE_COLUMN_WIDTH_DP.dp),
            contentAlignment = Alignment.CenterEnd,
        ) {
            val tier = model?.intelligenceTier
            if (tier == null) {
                Text("—", color = MaterialTheme.colorScheme.onSurfaceVariant)
            } else {
                Icon(
                    painter = painterResource(intelligenceIconResource(tier)),
                    contentDescription = null,
                    modifier = Modifier.size(13.dp),
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        Spacer(Modifier.width(MODEL_PERFORMANCE_COLUMN_GAP_DP.dp))
        Text(
            text = formatModelLatencyMs(displayedModelLatencyMs(model, latencyOverrideMs)),
            modifier = Modifier.width(MODEL_LATENCY_COLUMN_WIDTH_DP.dp),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.End,
            maxLines = 1,
        )
    }
}

internal fun displayedModelLatencyMs(
    model: PresetModelDescriptor?,
    liveOverrideMs: Int?,
): Int? = liveOverrideMs ?: model?.typicalLatencyMs

@DrawableRes
internal fun intelligenceIconResource(tier: Int): Int = when (intelligenceStatIconName(tier)) {
    "stat_3" -> R.drawable.ms_stat_3
    "stat_2" -> R.drawable.ms_stat_2
    "stat_1" -> R.drawable.ms_stat_1
    "stat_minus_1" -> R.drawable.ms_stat_minus_1
    "stat_minus_2" -> R.drawable.ms_stat_minus_2
    else -> R.drawable.ms_stat_minus_3
}

internal fun intelligenceStatIconName(tier: Int): String = when (tier.coerceIn(1, 6)) {
    6 -> "stat_3"
    5 -> "stat_2"
    4 -> "stat_1"
    3 -> "stat_minus_1"
    2 -> "stat_minus_2"
    else -> "stat_minus_3"
}

internal fun formatModelLatencyMs(milliseconds: Int?): String {
    if (milliseconds == null) return "—"
    val tenths = (milliseconds + 50) / 100
    return if (tenths % 10 == 0) {
        "${tenths / 10}s"
    } else {
        "${tenths / 10}.${tenths % 10}s"
    }
}
