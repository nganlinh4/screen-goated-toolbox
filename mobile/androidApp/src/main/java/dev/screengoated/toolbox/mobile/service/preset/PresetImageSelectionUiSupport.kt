package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import android.content.res.Configuration
import android.graphics.Canvas
import android.graphics.ColorFilter
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.Path
import android.graphics.Rect
import android.graphics.RectF
import android.os.Build
import android.graphics.Typeface
import android.graphics.PixelFormat
import android.graphics.drawable.Drawable
import android.view.WindowInsets
import android.view.WindowManager
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.MaterialShapes
import androidx.core.content.res.ResourcesCompat
import androidx.graphics.shapes.toPath
import dev.screengoated.toolbox.mobile.R

internal fun condensedRoundedTypeface(context: Context): Typeface? {
    return ResourcesCompat.getFont(context, R.font.google_sans_flex)
}

internal fun puffyBackground(
    fillColor: Int,
    strokeColor: Int,
    strokeWidthPx: Float,
): Drawable {
    return PuffyShapeDrawable(
        fillColor = fillColor,
        strokeColor = strokeColor,
        strokeWidthPx = strokeWidthPx,
    )
}

internal fun estimatedSystemBarInsets(
    context: Context,
    windowManager: WindowManager,
): Rect {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
        return windowManager.currentWindowMetrics
            .windowInsets
            .getInsetsIgnoringVisibility(
                WindowInsets.Type.systemBars() or WindowInsets.Type.displayCutout(),
            )
            .let { Rect(it.left, it.top, it.right, it.bottom) }
    }

    val resources = context.resources
    val orientation = resources.configuration.orientation
    val density = resources.displayMetrics.density
    val statusBar = (24f * density).toInt()
    val navigationBar = (48f * density).toInt()
    return if (orientation == Configuration.ORIENTATION_LANDSCAPE) {
        Rect(
            0,
            statusBar,
            navigationBar,
            0,
        )
    } else {
        Rect(
            0,
            statusBar,
            0,
            navigationBar,
        )
    }
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

@OptIn(ExperimentalMaterial3ExpressiveApi::class)
private class PuffyShapeDrawable(
    fillColor: Int,
    strokeColor: Int,
    strokeWidthPx: Float,
) : Drawable() {
    private val sourcePath = MaterialShapes.Puffy.toPath(Path())
    private val sourceBounds = RectF().also { sourcePath.computeBounds(it, true) }
    private val fillPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.FILL
        color = fillColor
    }
    private val strokePaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        color = strokeColor
        strokeWidth = strokeWidthPx.coerceAtLeast(1f)
    }
    private val path = Path()

    override fun onBoundsChange(bounds: Rect) {
        super.onBoundsChange(bounds)
        rebuildPath(bounds)
    }

    override fun draw(canvas: Canvas) {
        canvas.drawPath(path, fillPaint)
        canvas.drawPath(path, strokePaint)
    }

    override fun setAlpha(alpha: Int) {
        fillPaint.alpha = alpha
        strokePaint.alpha = alpha
        invalidateSelf()
    }

    @Deprecated("Drawable.setColorFilter is deprecated in the platform API")
    override fun setColorFilter(colorFilter: ColorFilter?) {
        fillPaint.colorFilter = colorFilter
        strokePaint.colorFilter = colorFilter
        invalidateSelf()
    }

    @Deprecated("Drawable.getOpacity is deprecated in the platform API")
    override fun getOpacity(): Int = PixelFormat.TRANSLUCENT

    private fun rebuildPath(bounds: Rect) {
        path.reset()
        if (bounds.isEmpty) return
        val rect = RectF(bounds)
        val inset = strokePaint.strokeWidth / 2f
        rect.inset(inset, inset)
        val matrix = Matrix()
        if (!matrix.setRectToRect(sourceBounds, rect, Matrix.ScaleToFit.FILL)) return
        path.set(sourcePath)
        path.transform(matrix)
    }
}
