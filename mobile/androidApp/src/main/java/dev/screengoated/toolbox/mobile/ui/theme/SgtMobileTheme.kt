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
import androidx.compose.material3.expressiveLightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.model.MobileThemeMode

private val DarkColors = darkColorScheme(
    primary = Color(0xFFF0B7FF),
    onPrimary = Color(0xFF4A1760),
    primaryContainer = Color(0xFF714087),
    onPrimaryContainer = Color(0xFFFFD8FA),
    secondary = Color(0xFFFFB2BE),
    onSecondary = Color(0xFF5C1328),
    secondaryContainer = Color(0xFF80364B),
    onSecondaryContainer = Color(0xFFFFD9E0),
    tertiary = Color(0xFF96E4D4),
    onTertiary = Color(0xFF063830),
    tertiaryContainer = Color(0xFF25554B),
    onTertiaryContainer = Color(0xFFB0F1E4),
    surface = Color(0xFF15111A),
    onSurface = Color(0xFFE6E1E9),
    surfaceContainer = Color(0xFF211B28),
    surfaceContainerHigh = Color(0xFF2B2433),
    surfaceContainerHighest = Color(0xFF38303F),
    surfaceContainerLow = Color(0xFF1A1520),
    surfaceContainerLowest = Color(0xFF110D15),
    surfaceBright = Color(0xFF433B4C),
    outline = Color(0xFFA49AAA),
    outlineVariant = Color(0xFF544C5C),
    error = Color(0xFFF2B8B5),
    onError = Color(0xFF601410),
    errorContainer = Color(0xFF8C1D18),
    onErrorContainer = Color(0xFFF9DEDC),
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
        isDark -> DarkColors
        else -> expressiveLightColorScheme()
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
