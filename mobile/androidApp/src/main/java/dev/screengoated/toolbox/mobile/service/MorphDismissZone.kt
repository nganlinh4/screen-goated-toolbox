package dev.screengoated.toolbox.mobile.service

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.Path
import android.graphics.Rect
import android.graphics.RectF
import android.os.Build
import android.view.Gravity
import android.view.View
import android.view.WindowManager
import android.view.animation.OvershootInterpolator
import android.widget.FrameLayout
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.MaterialShapes
import androidx.graphics.shapes.Morph
import androidx.graphics.shapes.toPath
import kotlin.math.roundToInt
import kotlin.math.sqrt

/**
 * Shared morphing dismiss-zone component used by BubbleService, PresetOverlay,
 * and HelpAssistantOverlay. Renders 1 or 2 targets at the screen bottom with
 * MaterialShapes morph + spin + proximity-driven color/scale + swallow animation.
 *
 * @param targets list of dismiss targets (1 = single center, 2 = center + left)
 */
internal class MorphDismissZone(
    private val context: Context,
    private val windowManager: WindowManager,
    private val targets: List<DismissTargetDef>,
) {
    private var container: FrameLayout? = null
    private val morphViews = mutableListOf<MorphShapeView>()
    private var prevClosestIdx = -1

    // Smoothed proximity values (EMA filter to prevent shaking)
    private val smoothedProximity = FloatArray(targets.size)

    fun show() {
        if (container != null) return
        val shapeSize = dp(SHAPE_SIZE_DP)
        // Layout cell is larger to give room for scale-up bloom without clipping
        val cellSize = dp(CELL_SIZE_DP)

        morphViews.clear()
        val root = FrameLayout(context).apply {
            clipChildren = false
            clipToPadding = false
        }

        targets.forEachIndexed { idx, def ->
            val view = MorphShapeView(context, def.morph, def.label, def.initialRotation, shapeSize)
            morphViews.add(view)
            root.addView(view, FrameLayout.LayoutParams(cellSize, cellSize).apply {
                gravity = def.gravity
                leftMargin = dp(def.leftMarginDp)
                rightMargin = dp(def.rightMarginDp)
                bottomMargin = dp(def.bottomMarginDp)
            })
        }

        val params = WindowManager.LayoutParams(
            WindowManager.LayoutParams.MATCH_PARENT,
            cellSize * 2,
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
                WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE or
                WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL,
            android.graphics.PixelFormat.TRANSLUCENT,
        ).apply {
            gravity = Gravity.BOTTOM or Gravity.START
        }
        container = root
        runCatching { windowManager.addView(root, params) }

        morphViews.forEachIndexed { idx, view ->
            view.animateIn(startDelay = idx * 60L)
        }
    }

    /**
     * Update all targets with raw proximity values. Applies EMA smoothing to
     * prevent shaking. Returns index of the closest target, or -1.
     */
    fun update(proximities: FloatArray): Int {
        if (container == null) return -1

        // EMA smoothing: α=0.35 — responsive but no jitter
        for (i in proximities.indices.take(morphViews.size)) {
            smoothedProximity[i] = smoothedProximity[i] * 0.65f + proximities[i] * 0.35f
            // Snap to 0 when very low to avoid lingering ghost scale
            if (smoothedProximity[i] < 0.02f) smoothedProximity[i] = 0f
        }

        var closestIdx = -1
        var closestVal = 0f
        for (i in morphViews.indices) {
            val p = smoothedProximity[i]
            morphViews[i].setMorphProgress(p)
            morphViews[i].setProximityFeedback(p)
            if (p > closestVal) {
                closestVal = p
                closestIdx = i
            }
        }

        // Spin on target change
        if (closestIdx != prevClosestIdx && closestIdx >= 0) {
            morphViews[closestIdx].animateSpin()
        }
        prevClosestIdx = closestIdx
        return closestIdx
    }

    /** Swallow animation on target [idx], then call [onDone]. */
    fun swallow(idx: Int, onDone: () -> Unit) {
        if (idx < 0 || idx >= morphViews.size) { onDone(); return }
        val target = morphViews[idx]
        target.animate()
            .scaleX(1.7f)
            .scaleY(1.7f)
            .setDuration(110)
            .withEndAction {
                target.animate()
                    .scaleX(0f)
                    .scaleY(0f)
                    .alpha(0f)
                    .setDuration(160)
                    .withEndAction {
                        hide()
                        onDone()
                    }
                    .start()
            }
            .start()
    }

    fun hide() {
        prevClosestIdx = -1
        smoothedProximity.fill(0f)
        val root = container ?: return
        root.animate()
            .alpha(0f)
            .setDuration(150)
            .withEndAction {
                runCatching { windowManager.removeView(root) }
                container = null
                morphViews.clear()
            }
            .start()
    }

    private fun dp(value: Int): Int = (value * context.resources.displayMetrics.density).roundToInt()

    // ── Target definition ───────────────────────────────────────────

    data class DismissTargetDef(
        val morph: Morph,
        val label: String,
        val initialRotation: Float = 0f,
        val gravity: Int = Gravity.BOTTOM or Gravity.CENTER_HORIZONTAL,
        val leftMarginDp: Int = 0,
        val rightMarginDp: Int = 0,
        val bottomMarginDp: Int = 24,
    )

    data class DismissHitResult(
        val proximities: FloatArray,
        val distanceSq: FloatArray,
    )

    companion object {
        const val SHAPE_SIZE_DP = 60
        const val CELL_SIZE_DP = 96
        private const val HIT_RADIUS_DP = 55f
        private const val OUTER_RADIUS_DP = 110f
        private const val APPROACHING_OUTER_RADIUS_DP = 140f

        /** Single-target zone: circle → cookie, center bottom. */
        @OptIn(ExperimentalMaterial3ExpressiveApi::class)
        fun singleDismiss(): List<DismissTargetDef> = listOf(
            DismissTargetDef(
                morph = Morph(MaterialShapes.Circle, MaterialShapes.Cookie9Sided),
                label = "×",
            ),
        )

        /** Two-target zone: center single + left all. */
        @OptIn(ExperimentalMaterial3ExpressiveApi::class)
        fun singleAndAll(allLabel: String): List<DismissTargetDef> = listOf(
            DismissTargetDef(
                morph = Morph(MaterialShapes.Circle, MaterialShapes.Cookie9Sided),
                label = "×",
            ),
            DismissTargetDef(
                morph = Morph(MaterialShapes.Diamond, MaterialShapes.Clover4Leaf),
                label = allLabel,
                initialRotation = 45f,
                gravity = Gravity.BOTTOM or Gravity.START,
                leftMarginDp = 0,
            ),
        )

        /** Compute raw proximity (0–1) from distance. */
        fun proximityFromDist(dist: Float, hitRadius: Float, outerRadius: Float): Float {
            return when {
                dist <= hitRadius -> 1f
                dist <= outerRadius -> 1f - (dist - hitRadius) / (outerRadius - hitRadius)
                else -> 0f
            }
        }

        fun hitTest(
            rawX: Float,
            rawY: Float,
            screenBounds: Rect,
            density: Float,
            coordinateScale: Float,
            targets: List<DismissTargetDef>,
            previousDistanceSq: FloatArray? = null,
        ): DismissHitResult {
            val proximities = FloatArray(targets.size)
            val distanceSq = FloatArray(targets.size)

            targets.forEachIndexed { idx, target ->
                val center = targetCenter(
                    screenBounds = screenBounds,
                    density = density,
                    coordinateScale = coordinateScale,
                    target = target,
                )
                val dx = rawX - center.first
                val dy = rawY - center.second
                val distSqValue = dx * dx + dy * dy
                val approaching = distSqValue < (previousDistanceSq?.getOrNull(idx) ?: Float.POSITIVE_INFINITY)
                val hitRadius = scaledDp(HIT_RADIUS_DP, density, coordinateScale)
                val outerRadius = scaledDp(
                    if (approaching) APPROACHING_OUTER_RADIUS_DP else OUTER_RADIUS_DP,
                    density,
                    coordinateScale,
                )
                val dist = sqrt(distSqValue)
                proximities[idx] = proximityFromDist(dist, hitRadius, outerRadius)
                distanceSq[idx] = distSqValue
            }

            return DismissHitResult(
                proximities = proximities,
                distanceSq = distanceSq,
            )
        }

        private fun targetCenter(
            screenBounds: Rect,
            density: Float,
            coordinateScale: Float,
            target: DismissTargetDef,
        ): Pair<Float, Float> {
            val unitScale = coordinateScale.takeIf { it > 0f } ?: 1f
            val cellSize = scaledDp(CELL_SIZE_DP.toFloat(), density, unitScale)
            val bottomMargin = scaledDp(target.bottomMarginDp.toFloat(), density, unitScale)
            val leftMargin = scaledDp(target.leftMarginDp.toFloat(), density, unitScale)
            val rightMargin = scaledDp(target.rightMarginDp.toFloat(), density, unitScale)
            val screenLeft = screenBounds.left / unitScale
            val screenRight = screenBounds.right / unitScale
            val absoluteGravity = Gravity.getAbsoluteGravity(target.gravity, View.LAYOUT_DIRECTION_LTR)
            val centerX = when (absoluteGravity and Gravity.HORIZONTAL_GRAVITY_MASK) {
                Gravity.LEFT -> screenLeft + leftMargin + (cellSize / 2f)
                Gravity.RIGHT -> screenRight - rightMargin - (cellSize / 2f)
                else -> screenBounds.exactCenterX() / unitScale
            }
            val centerY = (screenBounds.bottom / unitScale) - bottomMargin - (cellSize / 2f)
            return centerX to centerY
        }

        private fun scaledDp(value: Float, density: Float, coordinateScale: Float): Float {
            return value * density / coordinateScale
        }
    }

    // ── MorphShapeView ──────────────────────────────────────────────

    private class MorphShapeView(
        context: Context,
        private val morph: Morph,
        private val label: String,
        private val initialRotation: Float,
        private val shapeSize: Int,
    ) : View(context) {

        private var morphProgress = 0f
        // Track the shape's cumulative rotation separately from View.rotation
        // so the text never rotates.
        private var shapeRotation = initialRotation
        private val shapePaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            style = Paint.Style.FILL
            color = Color.argb(200, 60, 60, 60)
        }
        private val labelPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            textAlign = Paint.Align.CENTER
            color = Color.WHITE
            textSize = 20f * context.resources.displayMetrics.density
            val tf = condensedRoundedTypeface(context)
            if (tf != null) typeface = tf
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                fontVariationSettings = "'wght' 615, 'wdth' 68, 'ROND' 100"
            }
        }
        private val shapePath = Path()
        private val pathMatrix = Matrix()

        init {
            alpha = 0f
            scaleX = 0.3f
            scaleY = 0.3f
            // Do NOT set View.rotation — we rotate the shape path manually
        }

        fun setMorphProgress(progress: Float) {
            val clamped = progress.coerceIn(0f, 1f)
            if (morphProgress != clamped) {
                morphProgress = clamped
                invalidate()
            }
        }

        fun setProximityFeedback(proximity: Float) {
            val s = 1f + proximity * 0.4f
            scaleX = s
            scaleY = s
            val r = (60 + (180 * proximity)).toInt().coerceIn(0, 255)
            val g = (60 - (20 * proximity)).toInt().coerceIn(0, 255)
            val b = (60 - (20 * proximity)).toInt().coerceIn(0, 255)
            val a = (200 + (30 * proximity)).toInt().coerceIn(0, 255)
            shapePaint.color = Color.argb(a, r, g, b)
        }

        fun animateIn(startDelay: Long = 0) {
            animate()
                .alpha(1f)
                .scaleX(1f)
                .scaleY(1f)
                .setStartDelay(startDelay)
                .setDuration(300)
                .setInterpolator(OvershootInterpolator(1.5f))
                .start()
        }

        fun animateSpin() {
            // Animate shapeRotation only — text stays level
            val startRot = shapeRotation
            val endRot = startRot + 90f
            val anim = android.animation.ValueAnimator.ofFloat(startRot, endRot)
            anim.duration = 350
            anim.interpolator = OvershootInterpolator(1.2f)
            anim.addUpdateListener {
                shapeRotation = it.animatedValue as Float
                invalidate()
            }
            anim.start()
        }

        override fun onDraw(canvas: Canvas) {
            super.onDraw(canvas)
            val w = width.toFloat()
            val h = height.toFloat()
            if (w <= 0f || h <= 0f) return
            val cx = w / 2f
            val cy = h / 2f
            val s = shapeSize.toFloat()

            // Build morphed path scaled to shapeSize, centered in the (larger) view
            shapePath.rewind()
            morph.toPath(morphProgress, shapePath)
            val srcBounds = RectF()
            shapePath.computeBounds(srcBounds, true)
            if (srcBounds.isEmpty) return
            val inset = (w - s) / 2f
            val dstBounds = RectF(inset, (h - s) / 2f, inset + s, (h + s) / 2f)
            pathMatrix.reset()
            if (pathMatrix.setRectToRect(srcBounds, dstBounds, Matrix.ScaleToFit.CENTER)) {
                shapePath.transform(pathMatrix)
            }

            // Rotate shape only (not the whole canvas)
            val rotSave = canvas.save()
            canvas.rotate(shapeRotation, cx, cy)
            canvas.drawPath(shapePath, shapePaint)
            canvas.restoreToCount(rotSave)

            // Draw text un-rotated
            val textY = cy - (labelPaint.descent() + labelPaint.ascent()) / 2f
            canvas.drawText(label, cx, textY, labelPaint)
        }
    }
}

/** Load Google Sans Flex with condensed rounded settings. Shared with PresetImageSelectionUiSupport. */
private fun condensedRoundedTypeface(context: Context): android.graphics.Typeface? {
    return androidx.core.content.res.ResourcesCompat.getFont(
        context,
        dev.screengoated.toolbox.mobile.R.font.google_sans_flex,
    )
}
