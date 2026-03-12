@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui.theme

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.MotionScheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp

private val DarkColors = darkColorScheme()
private val LightColors = lightColorScheme()

private val SgtShapes = Shapes(
    extraSmall = RoundedCornerShape(16.dp),
    small = RoundedCornerShape(22.dp),
    medium = RoundedCornerShape(28.dp),
    large = RoundedCornerShape(36.dp),
    extraLarge = RoundedCornerShape(48.dp),
    largeIncreased = RoundedCornerShape(42.dp),
    extraLargeIncreased = RoundedCornerShape(58.dp),
    extraExtraLarge = RoundedCornerShape(72.dp),
)

@Composable
fun SgtMobileTheme(
    content: @Composable () -> Unit,
) {
    val context = LocalContext.current
    val isDark = isSystemInDarkTheme()
    val colors = when {
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && isDark -> dynamicDarkColorScheme(context)
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> dynamicLightColorScheme(context)
        isDark -> DarkColors
        else -> LightColors
    }

    MaterialTheme(
        colorScheme = colors,
        motionScheme = MotionScheme.expressive(),
        typography = SgtTypography,
        shapes = SgtShapes,
        content = content,
    )
}
