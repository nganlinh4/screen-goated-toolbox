@file:OptIn(ExperimentalMaterial3ExpressiveApi::class)

package dev.screengoated.toolbox.mobile.ui.theme

import android.os.Build
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Typography
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import dev.screengoated.toolbox.mobile.R

@OptIn(ExperimentalTextApi::class)
private fun roundedGoogleSansFlex(weight: FontWeight): Font {
    return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
        Font(
            resId = R.font.google_sans_flex,
            weight = weight,
            variationSettings = FontVariation.Settings(
                FontVariation.weight(weight.weight),
                FontVariation.Setting("ROND", 100f),
                FontVariation.Setting("ROUN", 100f),
                FontVariation.Setting("RNDS", 100f),
            ),
        )
    } else {
        Font(R.font.google_sans_flex, weight)
    }
}

private val GoogleSansFlexFamily = FontFamily(
    roundedGoogleSansFlex(FontWeight.Normal),
    roundedGoogleSansFlex(FontWeight.Medium),
    roundedGoogleSansFlex(FontWeight.SemiBold),
    roundedGoogleSansFlex(FontWeight.Bold),
    roundedGoogleSansFlex(FontWeight.ExtraBold),
    roundedGoogleSansFlex(FontWeight.Black),
)

val SgtTypography = Typography(
    displayLarge = TextStyle(
        fontFamily = GoogleSansFlexFamily,
        fontWeight = FontWeight.Black,
        fontSize = 56.sp,
        lineHeight = 60.sp,
        letterSpacing = (-0.8).sp,
    ),
    displayMedium = TextStyle(
        fontFamily = GoogleSansFlexFamily,
        fontWeight = FontWeight.Black,
        fontSize = 42.sp,
        lineHeight = 46.sp,
        letterSpacing = (-0.5).sp,
    ),
    headlineLarge = TextStyle(
        fontFamily = GoogleSansFlexFamily,
        fontWeight = FontWeight.Bold,
        fontSize = 30.sp,
        lineHeight = 34.sp,
    ),
    headlineMedium = TextStyle(
        fontFamily = GoogleSansFlexFamily,
        fontWeight = FontWeight.Bold,
        fontSize = 24.sp,
        lineHeight = 28.sp,
    ),
    titleLarge = TextStyle(
        fontFamily = GoogleSansFlexFamily,
        fontWeight = FontWeight.SemiBold,
        fontSize = 20.sp,
        lineHeight = 24.sp,
    ),
    bodyLarge = TextStyle(
        fontFamily = GoogleSansFlexFamily,
        fontWeight = FontWeight.Normal,
        fontSize = 16.sp,
        lineHeight = 22.sp,
    ),
    bodyMedium = TextStyle(
        fontFamily = GoogleSansFlexFamily,
        fontWeight = FontWeight.Medium,
        fontSize = 14.sp,
        lineHeight = 20.sp,
    ),
    labelLarge = TextStyle(
        fontFamily = GoogleSansFlexFamily,
        fontWeight = FontWeight.Bold,
        fontSize = 14.sp,
        letterSpacing = 0.2.sp,
    ),
    displayLargeEmphasized = TextStyle(
        fontFamily = GoogleSansFlexFamily,
        fontWeight = FontWeight.Black,
        fontSize = 58.sp,
        lineHeight = 62.sp,
        letterSpacing = (-1.0).sp,
    ),
    displayMediumEmphasized = TextStyle(
        fontFamily = GoogleSansFlexFamily,
        fontWeight = FontWeight.Black,
        fontSize = 46.sp,
        lineHeight = 50.sp,
        letterSpacing = (-0.6).sp,
    ),
    headlineSmallEmphasized = TextStyle(
        fontFamily = GoogleSansFlexFamily,
        fontWeight = FontWeight.ExtraBold,
        fontSize = 28.sp,
        lineHeight = 32.sp,
    ),
    titleLargeEmphasized = TextStyle(
        fontFamily = GoogleSansFlexFamily,
        fontWeight = FontWeight.Bold,
        fontSize = 22.sp,
        lineHeight = 28.sp,
    ),
    bodyLargeEmphasized = TextStyle(
        fontFamily = GoogleSansFlexFamily,
        fontWeight = FontWeight.SemiBold,
        fontSize = 16.sp,
        lineHeight = 24.sp,
    ),
    labelLargeEmphasized = TextStyle(
        fontFamily = GoogleSansFlexFamily,
        fontWeight = FontWeight.ExtraBold,
        fontSize = 14.sp,
        letterSpacing = 0.25.sp,
    ),
    labelMediumEmphasized = TextStyle(
        fontFamily = GoogleSansFlexFamily,
        fontWeight = FontWeight.Bold,
        fontSize = 12.sp,
        lineHeight = 16.sp,
        letterSpacing = 0.2.sp,
    ),
)
