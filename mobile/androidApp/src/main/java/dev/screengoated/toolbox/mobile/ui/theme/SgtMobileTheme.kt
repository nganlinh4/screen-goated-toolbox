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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.model.MobileThemeMode

private val DarkColors = darkColorScheme(
    primary = Color(0xFFD0BCFF),
    onPrimary = Color(0xFF381E72),
    primaryContainer = Color(0xFF4F378B),
    onPrimaryContainer = Color(0xFFEADDFF),
    secondary = Color(0xFFFFB4A8),
    onSecondary = Color(0xFF561E19),
    secondaryContainer = Color(0xFF73342D),
    onSecondaryContainer = Color(0xFFFFDAD6),
    tertiary = Color(0xFF7EDAC0),
    onTertiary = Color(0xFF003731),
    tertiaryContainer = Color(0xFF1B5049),
    onTertiaryContainer = Color(0xFFA0F0DC),
    surface = Color(0xFF12111A),
    onSurface = Color(0xFFE6E1E9),
    surfaceContainer = Color(0xFF1D1B25),
    surfaceContainerHigh = Color(0xFF282630),
    surfaceContainerHighest = Color(0xFF33313B),
    surfaceContainerLow = Color(0xFF171520),
    surfaceContainerLowest = Color(0xFF0E0D14),
    surfaceBright = Color(0xFF3B3841),
)

private val LightColors = lightColorScheme(
    primary = Color(0xFF6750A4),
    primaryContainer = Color(0xFFEADDFF),
    secondary = Color(0xFF8C4A3F),
    secondaryContainer = Color(0xFFFFDAD6),
    tertiary = Color(0xFF006B5A),
    tertiaryContainer = Color(0xFFA0F0DC),
)

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
    themeMode: MobileThemeMode,
    content: @Composable () -> Unit,
) {
    val context = LocalContext.current
    val isDark = when (themeMode) {
        MobileThemeMode.SYSTEM -> isSystemInDarkTheme()
        MobileThemeMode.DARK -> true
        MobileThemeMode.LIGHT -> false
    }
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
