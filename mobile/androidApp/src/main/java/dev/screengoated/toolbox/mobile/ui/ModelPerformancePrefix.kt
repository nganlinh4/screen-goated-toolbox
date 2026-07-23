package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.preset.PresetModelDescriptor

@Composable
internal fun ModelPerformancePrefix(
    model: PresetModelDescriptor,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier.width(116.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(modifier = Modifier.width(74.dp), contentAlignment = Alignment.CenterStart) {
            val tier = model.qualityTier
            if (tier == null) {
                Text("—", color = MaterialTheme.colorScheme.onSurfaceVariant)
            } else {
                Row(
                    horizontalArrangement = Arrangement.spacedBy(1.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    repeat(tier.coerceIn(1, 5)) {
                        Icon(
                            painter = painterResource(R.drawable.ms_psychology),
                            contentDescription = null,
                            modifier = Modifier.size(13.dp),
                            tint = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
        }
        Text(
            text = formatModelLatencyMs(model.typicalLatencyMs),
            modifier = Modifier.width(42.dp),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
        )
    }
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
