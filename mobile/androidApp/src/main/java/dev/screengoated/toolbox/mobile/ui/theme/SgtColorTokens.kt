package dev.screengoated.toolbox.mobile.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color

/**
 * Extended semantic color tokens that sit alongside [MaterialTheme.colorScheme].
 *
 * These cover domain-specific colors (overlay chrome, status indicators, categories,
 * carousel accents) that are not part of the core M3 color roles but still need
 * light/dark theme awareness.
 */
@Immutable
data class SgtExtendedColors(
    // ── Overlay chrome ──────────────────────────────────────────────
    val overlayBackground: Color,
    val overlayTextActive: Color,
    val overlayTextInactive: Color,
    val overlayCommittedText: Color,
    val overlayIconTint: Color,
    val overlayActionButtonBg: Color,
    val overlayResizeHandle: Color,

    // ── Overlay accents ─────────────────────────────────────────────
    val overlayListeningAccent: Color,
    val overlaySubtitleActiveTint: Color,
    val overlayTranslateActiveTint: Color,
    val overlayTranslationAccent: Color,
    val overlayTranslationTitle: Color,
    val overlayActiveButtonText: Color,
    val overlayInactiveButtonText: Color,

    // ── Waveform gradient ───────────────────────────────────────────
    val waveformGradientStart: Color,
    val waveformGradientEnd: Color,

    // ── Status indicators ───────────────────────────────────────────
    val statusProcessing: Color,
    val statusSuccess: Color,
    val statusWarning: Color,

    // ── Preset categories ───────────────────────────────────────────
    val categoryImage: Color,
    val categoryTextSelect: Color,
    val categoryTextInput: Color,
    val categoryMic: Color,
    val categoryDevice: Color,

    // ── Favorite star ───────────────────────────────────────────────
    val favoriteStar: Color,

    // ── Theme / language morph-toggle accents ────────────────────────
    val themeAutoColor: Color,
    val themeDarkColor: Color,
    val themeLightColor: Color,
    val langEnColor: Color,
    val langViColor: Color,
    val langKoColor: Color,

    // ── App carousel slot accents ───────────────────────────────────
    val appSlotTeal: Color,
    val appSlotCoral: Color,
    val appSlotPurple: Color,
    val appSlotAmber: Color,
    val appSlotBlue: Color,
)

// ── Light palette ───────────────────────────────────────────────────
fun lightSgtExtendedColors() = SgtExtendedColors(
    // Overlay chrome
    overlayBackground = Color(0xFFF8F2F8),
    overlayTextActive = Color(0xFF2A252E),
    overlayTextInactive = Color(0xFF86808A),
    overlayCommittedText = Color(0xFF8B858F),
    overlayIconTint = Color(0xFF8B8A90),
    overlayActionButtonBg = Color(0xFFF1EDF3),
    overlayResizeHandle = Color(0xFF9A949E),

    // Overlay accents
    overlayListeningAccent = Color(0xFF00C8FF),
    overlaySubtitleActiveTint = Color(0xFF2B78FF),
    overlayTranslateActiveTint = Color(0xFFE6005A),
    overlayTranslationAccent = Color(0xFFFF9633),
    overlayTranslationTitle = Color(0xFF6B656E),
    overlayActiveButtonText = Color(0xFF3D3940),
    overlayInactiveButtonText = Color(0xFF9A949E),

    // Waveform
    waveformGradientStart = Color(0xFF49DFFF),
    waveformGradientEnd = Color(0xFF00B9F5),

    // Status
    statusProcessing = Color(0xFF5C9CE6),
    statusSuccess = Color(0xFF5DB882),
    statusWarning = Color(0xFFDCA850),

    // Categories
    categoryImage = Color(0xFF5B8DEF),
    categoryTextSelect = Color(0xFFAB68E8),
    categoryTextInput = Color(0xFF42A5F5),
    categoryMic = Color(0xFFEF5350),
    categoryDevice = Color(0xFF66BB6A),

    // Favorite
    favoriteStar = Color(0xFFFFC107),

    // Theme/language toggles
    themeAutoColor = Color(0xFF8AB4F8),
    themeDarkColor = Color(0xFFD0BCFF),
    themeLightColor = Color(0xFFFFCC80),
    langEnColor = Color(0xFF82B1FF),
    langViColor = Color(0xFFFF8A80),
    langKoColor = Color(0xFF69F0AE),

    // App carousel slots
    appSlotTeal = Color(0xFF4DB6AC),
    appSlotCoral = Color(0xFFEF9A9A),
    appSlotPurple = Color(0xFFCE93D8),
    appSlotAmber = Color(0xFFFFCC80),
    appSlotBlue = Color(0xFF90CAF9),
)

// ── Dark palette ────────────────────────────────────────────────────
fun darkSgtExtendedColors() = SgtExtendedColors(
    // Overlay chrome — swap to dark surfaces
    overlayBackground = Color(0xFF282630),
    overlayTextActive = Color(0xFFE6E1E9),
    overlayTextInactive = Color(0xFF9A949E),
    overlayCommittedText = Color(0xFF9A949E),
    overlayIconTint = Color(0xFFA8A4AD),
    overlayActionButtonBg = Color(0xFF33313B),
    overlayResizeHandle = Color(0xFFA8A4AD),

    // Overlay accents — slightly brighter for dark bg contrast
    overlayListeningAccent = Color(0xFF40D4FF),
    overlaySubtitleActiveTint = Color(0xFF5C9AFF),
    overlayTranslateActiveTint = Color(0xFFFF4081),
    overlayTranslationAccent = Color(0xFFFFAB40),
    overlayTranslationTitle = Color(0xFFB0AAB5),
    overlayActiveButtonText = Color(0xFFE6E1E9),
    overlayInactiveButtonText = Color(0xFF7A757F),

    // Waveform — same vivid gradient works on dark
    waveformGradientStart = Color(0xFF49DFFF),
    waveformGradientEnd = Color(0xFF00B9F5),

    // Status — slightly lighter for dark bg readability
    statusProcessing = Color(0xFF7AB4F0),
    statusSuccess = Color(0xFF7DD09A),
    statusWarning = Color(0xFFECC06A),

    // Categories — slightly lighter
    categoryImage = Color(0xFF7BAAFF),
    categoryTextSelect = Color(0xFFC48AFF),
    categoryTextInput = Color(0xFF64B5F6),
    categoryMic = Color(0xFFEF7070),
    categoryDevice = Color(0xFF81C784),

    // Favorite
    favoriteStar = Color(0xFFFFD54F),

    // Theme/language toggles — same pastel works on dark
    themeAutoColor = Color(0xFF8AB4F8),
    themeDarkColor = Color(0xFFD0BCFF),
    themeLightColor = Color(0xFFFFCC80),
    langEnColor = Color(0xFF82B1FF),
    langViColor = Color(0xFFFF8A80),
    langKoColor = Color(0xFF69F0AE),

    // App carousel slots — same pastels work on dark
    appSlotTeal = Color(0xFF4DB6AC),
    appSlotCoral = Color(0xFFEF9A9A),
    appSlotPurple = Color(0xFFCE93D8),
    appSlotAmber = Color(0xFFFFCC80),
    appSlotBlue = Color(0xFF90CAF9),
)

val LocalSgtColors = staticCompositionLocalOf { lightSgtExtendedColors() }

/** Convenience accessor: `MaterialTheme.sgtColors.statusProcessing` etc. */
val MaterialTheme.sgtColors: SgtExtendedColors
    @Composable @ReadOnlyComposable
    get() = LocalSgtColors.current
