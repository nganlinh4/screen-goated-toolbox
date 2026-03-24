@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui.theme

import android.os.Build
import androidx.activity.enableEdgeToEdge
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
import androidx.compose.runtime.CompositionLocalProvider
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
    outline = Color(0xFF938F99),
    outlineVariant = Color(0xFF49454F),
    error = Color(0xFFF2B8B5),
    onError = Color(0xFF601410),
    errorContainer = Color(0xFF8C1D18),
    onErrorContainer = Color(0xFFF9DEDC),
)

private val LightColors = lightColorScheme(
    primary = Color(0xFF6750A4),
    onPrimary = Color(0xFFFFFFFF),
    primaryContainer = Color(0xFFEADDFF),
    onPrimaryContainer = Color(0xFF21005D),
    secondary = Color(0xFF8C4A3F),
    onSecondary = Color(0xFFFFFFFF),
    secondaryContainer = Color(0xFFFFDAD6),
    onSecondaryContainer = Color(0xFF3B0907),
    tertiary = Color(0xFF006B5A),
    onTertiary = Color(0xFFFFFFFF),
    tertiaryContainer = Color(0xFFA0F0DC),
    onTertiaryContainer = Color(0xFF00201A),
    surface = Color(0xFFFDF8FF),
    onSurface = Color(0xFF1C1B1F),
    surfaceContainer = Color(0xFFF0ECF4),
    surfaceContainerHigh = Color(0xFFEAE7EF),
    surfaceContainerHighest = Color(0xFFE5E1E9),
    surfaceContainerLow = Color(0xFFF6F2FA),
    surfaceContainerLowest = Color(0xFFFFFFFF),
    surfaceBright = Color(0xFFFDF8FF),
    outline = Color(0xFF79747E),
    outlineVariant = Color(0xFFCAC4D0),
    error = Color(0xFFB3261E),
    onError = Color(0xFFFFFFFF),
    errorContainer = Color(0xFFF9DEDC),
    onErrorContainer = Color(0xFF410E0B),
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

    // Update status bar icons to match theme (light icons on dark, dark icons on light)
    val activity = context as? androidx.activity.ComponentActivity
    if (activity != null) {
        androidx.compose.runtime.LaunchedEffect(isDark) {
            activity.enableEdgeToEdge(
                statusBarStyle = if (isDark) {
                    androidx.activity.SystemBarStyle.dark(android.graphics.Color.TRANSPARENT)
                } else {
                    androidx.activity.SystemBarStyle.light(android.graphics.Color.TRANSPARENT, android.graphics.Color.TRANSPARENT)
                },
                navigationBarStyle = if (isDark) {
                    androidx.activity.SystemBarStyle.dark(android.graphics.Color.TRANSPARENT)
                } else {
                    androidx.activity.SystemBarStyle.light(android.graphics.Color.TRANSPARENT, android.graphics.Color.TRANSPARENT)
                },
            )
        }
    }

    val sgtExtended = if (isDark) darkSgtExtendedColors() else lightSgtExtendedColors()

    CompositionLocalProvider(LocalSgtColors provides sgtExtended) {
        MaterialTheme(
            colorScheme = colors,
            motionScheme = MotionScheme.expressive(),
            typography = SgtTypography,
            shapes = SgtShapes,
            content = content,
        )
    }
}
