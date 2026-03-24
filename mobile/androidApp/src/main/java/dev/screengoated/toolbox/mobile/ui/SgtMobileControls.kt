@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui

import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialShapes
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.toPath
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Matrix
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.graphics.shapes.CornerRounding
import androidx.graphics.shapes.Morph
import androidx.graphics.shapes.RoundedPolygon
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.downloader.DownloaderViewModel
import dev.screengoated.toolbox.mobile.downloader.ui.DownloaderScreen
import dev.screengoated.toolbox.mobile.model.MobileThemeMode
import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import dev.screengoated.toolbox.mobile.ui.i18n.MobileUiLanguageOption
import dev.screengoated.toolbox.mobile.ui.theme.sgtColors

// Custom A shape: triangle silhouette with a V-notch at the bottom implying the legs + crossbar.
private val AShape = RoundedPolygon(
    vertices = floatArrayOf(
         0.00f, -1.00f,
         0.85f, 1.00f,
         0.22f, 0.05f,
         0.00f, 0.35f,
        -0.22f, 0.05f,
        -0.85f, 1.00f,
    ),
    perVertexRounding = listOf(
        CornerRounding(0.05f),
        CornerRounding(0.25f),
        CornerRounding(0.40f),
        CornerRounding(0.55f),
        CornerRounding(0.40f),
        CornerRounding(0.25f),
    ),
)

private val ThemeShapes = arrayOf(
    AShape,
    MaterialShapes.SemiCircle,
    MaterialShapes.Sunny,
)

private val ThemeColors
    @Composable get() = arrayOf(
        MaterialTheme.sgtColors.themeAutoColor,
        MaterialTheme.sgtColors.themeDarkColor,
        MaterialTheme.sgtColors.themeLightColor,
    )

private val ThemeRotations = floatArrayOf(0f, 120f, 240f)

@Composable
internal fun ThemeMorphToggle(
    themeMode: MobileThemeMode,
    onClick: () -> Unit,
    contentDescription: String,
) {
    val idx = themeMode.ordinal.coerceIn(0, 2)
    val morphProgress by animateFloatAsState(
        targetValue = idx.toFloat(),
        animationSpec = spring(
            dampingRatio = Spring.DampingRatioMediumBouncy,
            stiffness = Spring.StiffnessLow,
        ),
        label = "morph-progress",
    )
    val rotation by animateFloatAsState(
        targetValue = ThemeRotations[idx],
        animationSpec = spring(
            dampingRatio = Spring.DampingRatioMediumBouncy,
            stiffness = Spring.StiffnessLow,
        ),
        label = "morph-rotation",
    )
    val fromIdx = morphProgress.toInt().coerceIn(0, 1)
    val toIdx = (fromIdx + 1).coerceIn(0, 2)
    val segmentT = (morphProgress - fromIdx).coerceIn(0f, 1f)
    val morph = remember(fromIdx, toIdx) { Morph(ThemeShapes[fromIdx], ThemeShapes[toIdx]) }
    val color = lerpColor(ThemeColors[fromIdx], ThemeColors[toIdx], segmentT)

    IconButton(onClick = onClick) {
        Canvas(modifier = Modifier.size(28.dp)) {
            val path = morph.toPath(progress = segmentT)
            val s = size.minDimension
            val bounds = path.getBounds()
            val pathSize = maxOf(bounds.width, bounds.height).takeIf { it > 0f } ?: 1f
            val cx = bounds.left + bounds.width / 2f
            val cy = bounds.top + bounds.height / 2f
            val scale = s * 0.90f / pathSize
            val matrix = Matrix()
            matrix.translate(s / 2f, s / 2f)
            matrix.scale(scale, scale)
            matrix.rotateZ(rotation)
            matrix.translate(-cx, -cy)
            path.transform(matrix)
            drawPath(path, color = color)
        }
    }
}

private val EShape = RoundedPolygon(
    vertices = floatArrayOf(
        -1.00f, -1.00f,
         1.00f, -1.00f,
         1.00f, -0.45f,
        -0.10f, -0.45f,
        -0.10f, -0.08f,
         0.70f, -0.08f,
         0.70f, 0.08f,
        -0.10f, 0.08f,
        -0.10f, 0.45f,
         1.00f, 0.45f,
         1.00f, 1.00f,
        -1.00f, 1.00f,
    ),
    perVertexRounding = listOf(
        CornerRounding(0.15f), CornerRounding(0.15f), CornerRounding(0.15f),
        CornerRounding(0.08f), CornerRounding(0.08f), CornerRounding(0.15f),
        CornerRounding(0.15f), CornerRounding(0.08f), CornerRounding(0.08f),
        CornerRounding(0.15f), CornerRounding(0.15f), CornerRounding(0.15f),
    ),
)

private val VShape = RoundedPolygon(
    vertices = floatArrayOf(
        -0.90f, -1.00f,
         0.00f, 1.00f,
         0.90f, -1.00f,
         0.50f, -1.00f,
         0.00f, 0.18f,
        -0.50f, -1.00f,
    ),
    perVertexRounding = listOf(
        CornerRounding(0.20f),
        CornerRounding(0.04f),
        CornerRounding(0.20f),
        CornerRounding(0.20f),
        CornerRounding(0.45f),
        CornerRounding(0.20f),
    ),
)

private val KShape = RoundedPolygon(
    vertices = floatArrayOf(
        -1.00f, -1.00f,
        -0.25f, -1.00f,
         0.90f, -1.00f,
         0.05f, 0.00f,
         0.90f, 1.00f,
        -0.25f, 1.00f,
        -1.00f, 1.00f,
    ),
    perVertexRounding = listOf(
        CornerRounding(0.15f),
        CornerRounding(0.10f),
        CornerRounding(0.20f),
        CornerRounding(0.05f),
        CornerRounding(0.20f),
        CornerRounding(0.10f),
        CornerRounding(0.15f),
    ),
)

private val LanguageShapes = arrayOf(EShape, VShape, KShape)

private val LanguageColors
    @Composable get() = arrayOf(
        MaterialTheme.sgtColors.langEnColor,
        MaterialTheme.sgtColors.langViColor,
        MaterialTheme.sgtColors.langKoColor,
    )

private val LanguageRotations = floatArrayOf(0f, 0f, 0f)

@Composable
internal fun LanguageMorphToggle(
    uiLanguage: String,
    languageOptions: List<MobileUiLanguageOption>,
    onLanguageSelected: (String) -> Unit,
) {
    val idx = languageOptions.indexOfFirst { it.code == uiLanguage }.coerceAtLeast(0)
    val morphProgress by animateFloatAsState(
        targetValue = idx.toFloat(),
        animationSpec = spring(
            dampingRatio = Spring.DampingRatioMediumBouncy,
            stiffness = Spring.StiffnessLow,
        ),
        label = "lang-morph",
    )
    val rotation by animateFloatAsState(
        targetValue = LanguageRotations[idx.coerceIn(0, 2)],
        animationSpec = spring(
            dampingRatio = Spring.DampingRatioMediumBouncy,
            stiffness = Spring.StiffnessLow,
        ),
        label = "lang-rotation",
    )
    val fromIdx = morphProgress.toInt().coerceIn(0, LanguageShapes.size - 2)
    val toIdx = (fromIdx + 1).coerceIn(0, LanguageShapes.size - 1)
    val segmentT = (morphProgress - fromIdx).coerceIn(0f, 1f)
    val morph = remember(fromIdx, toIdx) { Morph(LanguageShapes[fromIdx], LanguageShapes[toIdx]) }
    val color = lerpColor(LanguageColors[fromIdx], LanguageColors[toIdx], segmentT)

    IconButton(
        onClick = {
            val next = languageOptions[(idx + 1) % languageOptions.size].code
            onLanguageSelected(next)
        },
    ) {
        Canvas(modifier = Modifier.size(28.dp)) {
            val path = morph.toPath(progress = segmentT)
            val s = size.minDimension
            val bounds = path.getBounds()
            val pathSize = maxOf(bounds.width, bounds.height).takeIf { it > 0f } ?: 1f
            val cx = bounds.left + bounds.width / 2f
            val cy = bounds.top + bounds.height / 2f
            val scale = s * 0.90f / pathSize
            val matrix = Matrix()
            matrix.translate(s / 2f, s / 2f)
            matrix.scale(scale, scale)
            matrix.rotateZ(rotation)
            matrix.translate(-cx, -cy)
            path.transform(matrix)
            drawPath(path, color = color)
        }
    }
}

private fun lerpColor(a: Color, b: Color, t: Float): Color = Color(
    red = a.red + (b.red - a.red) * t,
    green = a.green + (b.green - a.green) * t,
    blue = a.blue + (b.blue - a.blue) * t,
    alpha = 1f,
)

@Composable
internal fun DownloaderScreenWrapper(locale: MobileLocaleText, onBack: () -> Unit) {
    val context = LocalContext.current
    val app = context.applicationContext as SgtMobileApplication
    val vm: DownloaderViewModel = viewModel(
        factory = DownloaderViewModel.factory(app.appContainer.downloaderRepository),
    )
    DownloaderScreen(viewModel = vm, locale = locale, onBack = onBack)
}
