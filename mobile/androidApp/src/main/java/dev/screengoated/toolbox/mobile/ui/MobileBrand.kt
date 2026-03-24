package dev.screengoated.toolbox.mobile.ui

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.screengoated.toolbox.mobile.branding.MobileBrandAssets

@Composable
internal fun SgtBrandBadge(
    modifier: Modifier = Modifier,
    background: Color = MaterialTheme.colorScheme.surfaceContainerHighest,
    padding: Int = 10,
    size: Dp = 44.dp,
    showBackground: Boolean = true,
) {
    val isDarkSurface = background.luminance() < 0.45f
    val effectiveBackground = if (showBackground) background else Color.Transparent
    val boxModifier = if (showBackground) {
        modifier.size(size).clip(MaterialTheme.shapes.small).background(effectiveBackground).padding(padding.dp)
    } else {
        modifier.size(size)
    }
    Box(modifier = boxModifier) {
        Image(
            painter = painterResource(MobileBrandAssets.appIcon(isDarkSurface)),
            contentDescription = null,
            contentScale = ContentScale.Fit,
        )
    }
}
