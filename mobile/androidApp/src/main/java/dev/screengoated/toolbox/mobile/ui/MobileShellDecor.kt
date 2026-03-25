@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

internal object ShellSpacing {
    val sectionGap: Dp = 12.dp
    val cardGap: Dp = 16.dp
    val innerPad: Dp = 16.dp
    val itemGap: Dp = 12.dp
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
        color = accent.copy(alpha = 0.22f),
    ) {
        Text(
            text = label,
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
            style = MaterialTheme.typography.labelLargeEmphasized,
            color = accent,
        )
    }
}

internal fun methodLabel(
    locale: MobileLocaleText,
    method: MobileTtsMethod,
): String {
    return when (method) {
        MobileTtsMethod.GEMINI_LIVE -> "Gemini Live"
        MobileTtsMethod.EDGE_TTS -> "Edge TTS"
        MobileTtsMethod.GOOGLE_TRANSLATE -> "Google Trans."
    }
}
