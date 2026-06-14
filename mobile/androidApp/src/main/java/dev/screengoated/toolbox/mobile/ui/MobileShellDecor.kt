@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.model.androidSupportedMethod
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText

internal object ShellSpacing {
    val sectionGap: Dp = 12.dp
    val cardGap: Dp = 16.dp
    val innerPad: Dp = 16.dp
    val itemGap: Dp = 12.dp
}

@Composable
internal fun GradientMaskedIcon(
    @androidx.annotation.DrawableRes iconRes: Int,
    brush: Brush,
    modifier: Modifier = Modifier,
) {
    Icon(
        painter = painterResource(iconRes),
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

internal fun methodLabel(
    locale: MobileLocaleText,
    method: MobileTtsMethod,
): String {
    return when (method.androidSupportedMethod()) {
        MobileTtsMethod.GEMINI_LIVE -> locale.ttsMethodStandard
        MobileTtsMethod.EDGE_TTS -> locale.ttsMethodEdge
        MobileTtsMethod.GOOGLE_TRANSLATE -> locale.ttsMethodFast
    }
}

internal fun compactMethodLabel(
    locale: MobileLocaleText,
    method: MobileTtsMethod,
): String = methodLabel(locale, method)
    .substringBefore('(')
    .trim()
    .ifBlank { methodLabel(locale, method) }
