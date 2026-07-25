package dev.screengoated.toolbox.mobile.phonecontrol.overlay

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import kotlin.math.ceil

internal fun maskPhoneControlOverlay(bitmap: Bitmap): Bitmap {
    val sourceBounds = PhoneControlOverlayExclusion.currentCaptureBounds() ?: return bitmap
    val mask = controllerOverlayMaskBounds(sourceBounds, bitmap.width, bitmap.height)
        ?: return bitmap
    val output = if (bitmap.isMutable && bitmap.config == Bitmap.Config.ARGB_8888) {
        bitmap
    } else {
        bitmap.copy(Bitmap.Config.ARGB_8888, true) ?: return bitmap
    }
    Canvas(output).drawRect(
        mask.left.toFloat(),
        mask.top.toFloat(),
        mask.right.toFloat(),
        mask.bottom.toFloat(),
        Paint().apply { color = Color.BLACK },
    )
    if (output !== bitmap) bitmap.recycle()
    return output
}

internal fun controllerOverlayMaskBounds(
    overlay: OverlayBounds,
    bitmapWidth: Int,
    bitmapHeight: Int,
): OverlayBounds? {
    if (bitmapWidth <= 0 || bitmapHeight <= 0) return null
    val padding = ceil(
        minOf(
            (overlay.right - overlay.left).coerceAtLeast(0),
            (overlay.bottom - overlay.top).coerceAtLeast(0),
        ) * MASK_PADDING_FRACTION,
    ).toInt()
    val left = (overlay.left - padding).coerceIn(0, bitmapWidth)
    val top = (overlay.top - padding).coerceIn(0, bitmapHeight)
    val right = (overlay.right + padding).coerceIn(0, bitmapWidth)
    val bottom = (overlay.bottom + padding).coerceIn(0, bitmapHeight)
    return OverlayBounds(left, top, right, bottom)
        .takeIf { it.right > it.left && it.bottom > it.top }
}

private const val MASK_PADDING_FRACTION = 0.125
