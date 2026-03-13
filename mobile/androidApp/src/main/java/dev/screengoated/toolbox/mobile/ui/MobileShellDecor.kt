@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ElevatedCard
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.shared.live.SessionPhase
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import androidx.compose.ui.unit.dp

@Composable
internal fun SectionTile(
    label: String,
    description: String,
    icon: ImageVector,
    selected: Boolean,
    onClick: () -> Unit,
    brush: Brush,
) {
    val emphasis by animateFloatAsState(if (selected) 1f else 0f, label = "section-tile")
    ElevatedCard(
        modifier = Modifier
            .fillMaxWidth()
            .graphicsLayer {
                scaleX = 1f + emphasis * 0.02f
                scaleY = 1f + emphasis * 0.02f
            }
            .clickable(onClick = onClick),
        shape = MaterialTheme.shapes.extraLarge,
        colors = CardDefaults.elevatedCardColors(
            containerColor = if (selected) {
                MaterialTheme.colorScheme.surfaceContainerHighest
            } else {
                MaterialTheme.colorScheme.surfaceContainerLow
            },
        ),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(18.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Surface(
                modifier = Modifier.size(52.dp),
                shape = CircleShape,
                color = MaterialTheme.colorScheme.surfaceBright,
            ) {
                GradientMaskedIcon(icon, brush, modifier = Modifier.padding(13.dp))
            }
            Column(
                modifier = Modifier.weight(1f),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Text(text = label, style = MaterialTheme.typography.titleMedium)
                Text(
                    text = description,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

@Composable
internal fun GradientMaskedIcon(
    imageVector: ImageVector,
    brush: Brush,
    modifier: Modifier = Modifier,
) {
    Icon(
        imageVector = imageVector,
        contentDescription = null,
        modifier = modifier
            .graphicsLayer(alpha = 0.99f)
            .drawWithContent {
                drawContent()
                drawRect(brush = brush, blendMode = BlendMode.SrcIn)
            },
        tint = Color.White,
    )
}

@Composable
internal fun StatusChip(
    label: String,
    accent: Color,
) {
    Surface(
        shape = CircleShape,
        color = accent.copy(alpha = 0.14f),
    ) {
        Text(
            text = label,
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
            style = MaterialTheme.typography.labelLargeEmphasized,
            color = accent,
        )
    }
}

internal fun statusLabelForPhase(
    locale: MobileLocaleText,
    phase: SessionPhase,
): String {
    return when (phase) {
        SessionPhase.STARTING -> locale.shellStatusStarting
        SessionPhase.LISTENING -> locale.shellStatusActive
        SessionPhase.TRANSLATING -> locale.shellStatusTranslating
        else -> locale.shellStatusIdle
    }
}

internal fun methodLabel(
    locale: MobileLocaleText,
    method: MobileTtsMethod,
): String {
    return when (method) {
        MobileTtsMethod.GEMINI_LIVE -> locale.ttsMethodStandard
        MobileTtsMethod.EDGE_TTS -> locale.ttsMethodEdge
        MobileTtsMethod.GOOGLE_TRANSLATE -> locale.ttsMethodFast
    }
}
