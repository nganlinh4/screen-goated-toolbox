package dev.screengoated.toolbox.mobile.phonecontrol.overlay

import android.content.Context
import android.graphics.PixelFormat
import android.graphics.Rect
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.view.Choreographer
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.WindowManager
import dev.screengoated.toolbox.mobile.phonecontrol.GeneratedPhoneControlContract
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlOverlayStateSink
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlServiceState
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlActivity
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerChoice
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerPreferences
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import kotlin.coroutines.resume
import kotlin.math.abs

internal class PhoneControlOverlayController(
    private val context: Context,
) : PhoneControlOverlayStateSink, PhoneControlOverlayExclusionParticipant {
    private val mainHandler = Handler(Looper.getMainLooper())
    private val preferences = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
    private val captureState = OverlayCaptureGate()
    private val relocationState = Mutex()
    private val orbSize = context.dp(128)
    private val edgeMargin = context.dp(12)

    private var host = PhoneControlOverlayWindowHost.resolve(context)
    private var orb: PhoneControlOrbView? = null
    private var touchTarget: View? = null
    private var powerPrompt: PhoneControlPowerPromptView? = null
    private var orbParams: WindowManager.LayoutParams? = null
    private var touchParams: WindowManager.LayoutParams? = null
    private var powerPromptParams: WindowManager.LayoutParams? = null
    private var visual = PhoneControlOverlayVisual(
        GeneratedPhoneControlContract.ORB_STATE_IDLE,
        null,
        "",
        0f,
        false,
    )
    private var powerPromptVisible = PhoneControlPowerPreferences.current(context) == null
    private var appliedCaptureHidden: Boolean? = null
    private var destroyed = false

    @Volatile
    private var interactionBounds: Rect? = null

    override fun onState(state: PhoneControlServiceState) {
        val next = phoneControlOverlayVisual(state)
        mainHandler.post {
            if (!destroyed) {
                visual = next
                render()
            }
        }
    }

    fun onConfigurationChanged() {
        mainHandler.post {
            if (!destroyed) {
                positionFromFractions()
                render(forceLayout = true)
            }
        }
    }

    fun destroy() {
        if (Looper.myLooper() != Looper.getMainLooper()) {
            mainHandler.post(::destroy)
            return
        }
        if (destroyed) return
        destroyed = true
        detachWindows()
    }

    override fun orbBounds(): OverlayBounds? = interactionBounds?.let { bounds ->
        OverlayBounds(bounds.left, bounds.top, bounds.right, bounds.bottom)
    }

    override suspend fun <T> withOverlayHidden(block: suspend () -> T): T =
        captureState.withHidden(
            onHide = { firstCapture ->
                withContext(Dispatchers.Main.immediate) {
                    val wasVisible = firstCapture && orb != null && visual.visible
                    render()
                    if (wasVisible) awaitFrames(2)
                }
            },
            onRestore = { lastCapture ->
                if (lastCapture) {
                    withContext(Dispatchers.Main.immediate) { render() }
                }
            },
            block = block,
        )

    override suspend fun <T> withOverlayAvoiding(
        bounds: OverlayBounds,
        block: suspend () -> T,
    ): T = relocationState.withLock {
        val original = withContext(Dispatchers.Main.immediate) {
            val params = touchParams ?: return@withContext null
            val home = params.x to params.y
            val screen = screenBounds()
            val target = farthestOverlayCorner(
                OverlayBounds(screen.left, screen.top, screen.right, screen.bottom),
                params.width,
                params.height,
                edgeMargin,
                bounds,
            )
            params.x = target.first
            params.y = target.second
            params.flags = params.flags or WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE
            updateLayouts(persistPosition = false)
            awaitFrames(2)
            home
        }
        try {
            currentCoroutineContext().ensureActive()
            block()
        } finally {
            if (original != null) {
                withContext(NonCancellable) {
                    withContext(Dispatchers.Main.immediate) {
                        touchParams?.let { params ->
                            params.x = original.first
                            params.y = original.second
                            if (!captureState.isHidden) {
                                params.flags = params.flags and
                                    WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE.inv()
                            }
                            updateLayouts(persistPosition = false)
                        }
                    }
                }
            }
        }
    }

    private fun render(forceLayout: Boolean = false) {
        refreshHost()
        val canShow = visual.visible && host.isAvailable()
        if (!canShow) {
            detachWindows()
            return
        }
        val orbWasAttached = orb != null
        val promptWasAttached = powerPrompt != null
        ensureWindows()
        if (powerPromptVisible) ensurePowerPrompt() else detachPowerPrompt()
        val captureHidden = captureState.isHidden
        val suppressionChanged = appliedCaptureHidden != captureHidden
        appliedCaptureHidden = captureHidden
        applyWindowSuppression(if (captureHidden) 0f else 1f)
        val renderedVisual = if (powerPromptVisible) visual.copy(caption = "") else visual
        orb?.render(renderedVisual, currentPlacement())
        val windowSetChanged = !orbWasAttached || promptWasAttached != (powerPrompt != null)
        if (needsOverlayLayoutUpdate(forceLayout, windowSetChanged, suppressionChanged)) {
            updateLayouts()
        }
    }

    private fun refreshHost() {
        val needsRefresh = if (host.trusted) !host.isAvailable() else {
            dev.screengoated.toolbox.mobile.service.SgtAccessibilityService.instance != null
        }
        if (!needsRefresh) return
        val next = PhoneControlOverlayWindowHost.resolve(context)
        if (host.sameOwner(next)) return
        detachWindows()
        host = next
        PhoneControlLog.i(TAG, "overlay_host_changed host=${host.describe()}")
    }

    private fun applyWindowSuppression(alpha: Float) {
        orbParams?.alpha = alpha * host.rendererAlpha
        touchParams?.apply {
            this.alpha = alpha
            flags = if (captureState.isHidden) {
                flags or WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE
            } else {
                flags and WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE.inv()
            }
        }
        powerPromptParams?.apply {
            this.alpha = alpha
            flags = if (captureState.isHidden) {
                flags or WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE
            } else {
                flags and WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE.inv()
            }
        }
        touchTarget?.alpha = alpha
        powerPrompt?.alpha = alpha
    }

    private fun ensureWindows() {
        if (orb != null) return
        val orbView = PhoneControlOrbView(host.context, ::onRendererGone)
        val touchView = View(host.context).apply {
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS
            setOnTouchListener(OrbTouchListener())
        }
        val rendererLayout = overlayLayoutParams(
            width = WindowManager.LayoutParams.MATCH_PARENT,
            height = WindowManager.LayoutParams.MATCH_PARENT,
            flags = WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE or
                WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL or
                WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
        ).apply { configureFullDisplayLayout() }
        val touchLayout = overlayLayoutParams(
            width = orbSize,
            height = orbSize,
            flags = WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL or
                WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
        )
        orb = orbView
        touchTarget = touchView
        orbParams = rendererLayout
        touchParams = touchLayout
        positionFromFractions()
        runCatching {
            host.windowManager.addView(orbView, rendererLayout)
            host.windowManager.addView(touchView, touchLayout)
        }.onFailure {
            PhoneControlLog.e(TAG, "overlay_attach_failed host=${host.describe()}", it)
            detachWindows()
            return
        }
        PhoneControlLog.i(TAG, "overlay_attached host=${host.describe()}")
    }

    private fun ensurePowerPrompt() {
        if (powerPrompt != null) return
        val prompt = PhoneControlPowerPromptView(host.context, ::selectPowerChoice)
        val bounds = screenBounds()
        val params = overlayLayoutParams(
            width = minOf(context.dp(340), (bounds.width() - edgeMargin * 2).coerceAtLeast(1)),
            height = WindowManager.LayoutParams.WRAP_CONTENT,
            flags = WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL or
                WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
        )
        powerPrompt = prompt
        powerPromptParams = params
        runCatching { host.windowManager.addView(prompt, params) }
            .onFailure {
                PhoneControlLog.e(TAG, "power_prompt_attach_failed", it)
                detachPowerPrompt()
            }
    }

    private fun selectPowerChoice(choice: PhoneControlPowerChoice) {
        PhoneControlPowerPreferences.save(context, choice)
        PhoneControlLog.i(TAG, "power_choice choice=${choice.wireName}")
        powerPromptVisible = false
        render()
        if (choice != PhoneControlPowerChoice.STANDARD) {
            context.startActivity(PhoneControlActivity.optionalPowerIntent(context, choice))
        }
    }

    private fun togglePowerPrompt() {
        powerPromptVisible = !powerPromptVisible
        PhoneControlLog.i(TAG, "power_prompt visible=$powerPromptVisible")
        render()
    }

    private fun positionFromFractions() {
        val params = touchParams ?: return
        val bounds = screenBounds()
        val minX = bounds.left + edgeMargin
        val minY = bounds.top + edgeMargin
        val maxX = (bounds.right - params.width - edgeMargin).coerceAtLeast(minX)
        val maxY = (bounds.bottom - params.height - edgeMargin).coerceAtLeast(minY)
        params.x = minX + (
            preferences.getFloat(KEY_X_FRACTION, DEFAULT_X_FRACTION) * (maxX - minX)
            ).toInt()
        params.y = minY + (
            preferences.getFloat(KEY_Y_FRACTION, DEFAULT_Y_FRACTION) * (maxY - minY)
            ).toInt()
    }

    private fun clampAndSavePosition(persistPosition: Boolean = true) {
        val params = touchParams ?: return
        val bounds = screenBounds()
        val minX = bounds.left + edgeMargin
        val minY = bounds.top + edgeMargin
        val maxX = (bounds.right - params.width - edgeMargin).coerceAtLeast(minX)
        val maxY = (bounds.bottom - params.height - edgeMargin).coerceAtLeast(minY)
        params.x = params.x.coerceIn(minX, maxX)
        params.y = params.y.coerceIn(minY, maxY)
        if (persistPosition) {
            preferences.edit()
                .putFloat(KEY_X_FRACTION, (params.x - minX).toFloat() / (maxX - minX).coerceAtLeast(1))
                .putFloat(KEY_Y_FRACTION, (params.y - minY).toFloat() / (maxY - minY).coerceAtLeast(1))
                .apply()
        }
    }

    private fun updateLayouts(persistPosition: Boolean = true) {
        val orbView = orb ?: return
        val rendererLayout = orbParams ?: return
        val touchView = touchTarget ?: return
        val targetLayout = touchParams ?: return
        clampAndSavePosition(persistPosition)
        rendererLayout.configureFullDisplayLayout()
        runCatching { host.windowManager.updateViewLayout(orbView, rendererLayout) }
        runCatching { host.windowManager.updateViewLayout(touchView, targetLayout) }
        val renderedVisual = if (powerPromptVisible) visual.copy(caption = "") else visual
        orbView.render(renderedVisual, currentPlacement())

        val bounds = screenBounds()
        val promptBounds = powerPrompt?.let { prompt ->
            val params = powerPromptParams ?: return@let null
            val placeLeft = targetLayout.x + targetLayout.width / 2 > bounds.centerX()
            params.x = if (placeLeft) {
                (targetLayout.x + targetLayout.width - params.width)
                    .coerceAtLeast(bounds.left + edgeMargin)
            } else {
                targetLayout.x.coerceAtMost(bounds.right - params.width - edgeMargin)
            }
            val height = prompt.height.takeIf { it > 0 } ?: context.dp(170)
            val above = targetLayout.y - context.dp(10) - height
            params.y = if (above >= bounds.top + edgeMargin) {
                above
            } else {
                (targetLayout.y + targetLayout.height + context.dp(10))
                    .coerceAtMost(bounds.bottom - height - edgeMargin)
            }
            runCatching { host.windowManager.updateViewLayout(prompt, params) }
            Rect(params.x, params.y, params.x + params.width, params.y + height)
        }
        interactionBounds = if (!captureState.isHidden && visual.visible) {
            Rect(
                targetLayout.x,
                targetLayout.y,
                targetLayout.x + targetLayout.width,
                targetLayout.y + targetLayout.height,
            ).apply { promptBounds?.let(::union) }
        } else {
            null
        }
    }

    private fun currentPlacement(): PhoneControlOrbPlacement {
        val bounds = screenBounds()
        val params = checkNotNull(touchParams)
        val width = bounds.width().coerceAtLeast(1)
        val height = bounds.height().coerceAtLeast(1)
        return PhoneControlOrbPlacement(
            centerXFraction = (params.x - bounds.left + params.width / 2f) / width,
            centerYFraction = (params.y - bounds.top + params.height / 2f) / height,
            magnification = LOCAL_RENDERER_MAGNIFICATION * params.width / minOf(width, height),
        )
    }

    private fun onRendererGone(deadView: PhoneControlOrbView, crashed: Boolean) {
        if (destroyed || orb !== deadView) return
        PhoneControlLog.w(TAG, "renderer_recreate crashed=$crashed")
        detachWindows()
        render(forceLayout = true)
    }

    private fun detachWindows() {
        orb?.let { view ->
            runCatching { host.windowManager.removeView(view) }
            view.dispose()
        }
        touchTarget?.let { runCatching { host.windowManager.removeView(it) } }
        detachPowerPrompt()
        orb = null
        touchTarget = null
        orbParams = null
        touchParams = null
        interactionBounds = null
        appliedCaptureHidden = null
    }

    private fun detachPowerPrompt() {
        powerPrompt?.let { runCatching { host.windowManager.removeView(it) } }
        powerPrompt = null
        powerPromptParams = null
    }

    private fun overlayLayoutParams(width: Int, height: Int, flags: Int) =
        WindowManager.LayoutParams(
            width,
            height,
            host.windowType,
            flags,
            PixelFormat.TRANSLUCENT,
        ).apply { gravity = Gravity.TOP or Gravity.START }

    private fun WindowManager.LayoutParams.configureFullDisplayLayout() {
        val bounds = screenBounds()
        width = bounds.width()
        height = bounds.height()
        x = bounds.left
        y = bounds.top
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) fitInsetsTypes = 0
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            layoutInDisplayCutoutMode =
                WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS
        }
    }

    private fun screenBounds(): Rect = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
        host.windowManager.currentWindowMetrics.bounds
    } else {
        legacyScreenBounds()
    }

    @Suppress("DEPRECATION")
    private fun legacyScreenBounds(): Rect = Rect().also { bounds ->
        host.windowManager.defaultDisplay.getRectSize(bounds)
    }

    private suspend fun awaitFrames(count: Int) {
        repeat(count) {
            suspendCancellableCoroutine { continuation ->
                Choreographer.getInstance().postFrameCallback {
                    if (continuation.isActive) continuation.resume(Unit)
                }
            }
        }
    }

    private inner class OrbTouchListener : View.OnTouchListener {
        private var downX = 0f
        private var downY = 0f
        private var startX = 0
        private var startY = 0
        private var dragging = false

        override fun onTouch(view: View, event: MotionEvent): Boolean {
            val params = touchParams ?: return false
            when (event.actionMasked) {
                MotionEvent.ACTION_DOWN -> {
                    downX = event.rawX
                    downY = event.rawY
                    startX = params.x
                    startY = params.y
                    dragging = false
                    return true
                }
                MotionEvent.ACTION_MOVE -> {
                    val dx = event.rawX - downX
                    val dy = event.rawY - downY
                    if (!dragging && (abs(dx) > context.dp(5) || abs(dy) > context.dp(5))) {
                        dragging = true
                    }
                    if (dragging) {
                        params.x = startX + dx.toInt()
                        params.y = startY + dy.toInt()
                        updateLayouts()
                    }
                    return true
                }
                MotionEvent.ACTION_UP -> {
                    if (dragging) {
                        clampAndSavePosition()
                        updateLayouts()
                    } else {
                        view.performClick()
                        togglePowerPrompt()
                    }
                    return true
                }
                MotionEvent.ACTION_CANCEL -> return true
            }
            return false
        }
    }

    private companion object {
        const val TAG = "SGTPhoneControlOverlay"
        const val PREFS_NAME = "phone_control_overlay"
        const val KEY_X_FRACTION = "orb_x_fraction"
        const val KEY_Y_FRACTION = "orb_y_fraction"
        const val DEFAULT_X_FRACTION = 0.88f
        const val DEFAULT_Y_FRACTION = 0.28f
        const val LOCAL_RENDERER_MAGNIFICATION = 1.3f
    }
}

internal class OverlayCaptureGate {
    private val state = Mutex()

    @Volatile
    private var captureDepth = 0

    internal val depth: Int
        get() = captureDepth

    internal val isHidden: Boolean
        get() = captureDepth > 0

    internal suspend fun <T> withHidden(
        onHide: suspend (firstCapture: Boolean) -> Unit,
        onRestore: suspend (lastCapture: Boolean) -> Unit,
        block: suspend () -> T,
    ): T {
        var entered = false
        try {
            state.withLock {
                val firstCapture = captureDepth == 0
                captureDepth += 1
                entered = true
                onHide(firstCapture)
            }
            currentCoroutineContext().ensureActive()
            return block()
        } finally {
            if (entered) {
                withContext(NonCancellable) {
                    state.withLock {
                        check(captureDepth > 0) { "Overlay capture depth underflow" }
                        captureDepth -= 1
                        onRestore(captureDepth == 0)
                    }
                }
            }
        }
    }
}

private fun Context.dp(value: Int): Int = (value * resources.displayMetrics.density).toInt()

internal fun needsOverlayLayoutUpdate(
    forceLayout: Boolean,
    windowSetChanged: Boolean,
    suppressionChanged: Boolean,
): Boolean = forceLayout || windowSetChanged || suppressionChanged

internal fun farthestOverlayCorner(
    screen: OverlayBounds,
    overlayWidth: Int,
    overlayHeight: Int,
    margin: Int,
    avoid: OverlayBounds,
): Pair<Int, Int> {
    val left = screen.left + margin
    val top = screen.top + margin
    val right = (screen.right - overlayWidth - margin).coerceAtLeast(left)
    val bottom = (screen.bottom - overlayHeight - margin).coerceAtLeast(top)
    val avoidX = avoid.left.toLong() + (avoid.right - avoid.left) / 2L
    val avoidY = avoid.top.toLong() + (avoid.bottom - avoid.top) / 2L
    return listOf(left to top, right to top, left to bottom, right to bottom).maxBy { point ->
        val centerX = point.first.toLong() + overlayWidth / 2L
        val centerY = point.second.toLong() + overlayHeight / 2L
        val dx = centerX - avoidX
        val dy = centerY - avoidY
        dx * dx + dy * dy
    }
}
