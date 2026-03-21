package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import android.content.res.Configuration
import android.content.res.Resources
import android.graphics.Rect
import android.os.Build
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.view.WindowManager
import androidx.core.content.res.ResourcesCompat
import dev.screengoated.toolbox.mobile.R

internal fun condensedRoundedTypeface(context: Context): Typeface? {
    return ResourcesCompat.getFont(context, R.font.google_sans_flex)
}

internal fun pillBackground(
    fillColor: Int,
    strokeColor: Int,
    radiusPx: Float,
): GradientDrawable {
    return GradientDrawable().apply {
        shape = GradientDrawable.RECTANGLE
        cornerRadius = radiusPx
        setColor(fillColor)
        setStroke(1, strokeColor)
    }
}

internal fun estimatedSystemBarInsets(context: Context): Rect {
    val resources = context.resources
    val orientation = resources.configuration.orientation
    val statusBar = systemDimenPx(resources, "status_bar_height")
    return if (orientation == Configuration.ORIENTATION_LANDSCAPE) {
        Rect(
            0,
            statusBar,
            systemDimenPx(resources, "navigation_bar_width"),
            0,
        )
    } else {
        Rect(
            0,
            statusBar,
            0,
            systemDimenPx(resources, "navigation_bar_height"),
        )
    }
}

internal fun systemDimenPx(
    resources: Resources,
    name: String,
): Int {
    val id = resources.getIdentifier(name, "dimen", "android")
    return if (id > 0) resources.getDimensionPixelSize(id) else 0
}

internal fun fullOverlayBounds(
    context: Context,
    windowManager: WindowManager,
): Rect {
    return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
        windowManager.maximumWindowMetrics.bounds
    } else {
        val metrics = context.resources.displayMetrics
        Rect(0, 0, metrics.widthPixels, metrics.heightPixels)
    }
}
