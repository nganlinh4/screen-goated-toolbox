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
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.SgtAdbCommandBridge
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlActivity
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerChoice
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerPreferences
import dev.screengoated.toolbox.mobile.service.DismissAction
import dev.screengoated.toolbox.mobile.service.DismissBubbleController
import dev.screengoated.toolbox.mobile.service.overlay.overlayWebViewWindowFlags
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import kotlin.coroutines.resume

internal class PhoneControlOverlayController(
    private val context: Context,
    private val onDismiss: () -> Unit,
    private val onPowerChoiceSelected: (PhoneControlPowerChoice) -> Unit = {},
) : PhoneControlOverlayStateSink, PhoneControlOverlayExclusionParticipant {
    private val mainHandler = Handler(Looper.getMainLooper())
    private val preferences = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
    private val relocationState = Mutex()
    private val powerChoiceObserver = PhoneControlPowerPreferences.observe(context) {
        mainHandler.post {
            if (!destroyed) {
                detachPowerPrompt()
                if (powerPromptVisible) render()
            }
        }
    }
    private val orbSize = context.dp(128)
    private val edgeMargin = context.dp(12)
    private var host = PhoneControlOverlayWindowHost.resolve(context)
    private var dismissBubble = createDismissBubble(host)
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
    private var dismissing = false
    private var destroyed = false
    @Volatile
    private var interactionBounds: Rect? = null
    @Volatile
    private var rendererBounds: Rect? = null

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
        powerChoiceObserver.close()
        detachWindows()
    }

    fun orbBounds(): OverlayBounds? = interactionBounds?.let { bounds ->
        OverlayBounds(bounds.left, bounds.top, bounds.right, bounds.bottom)
    }

    override fun interactionBounds(): OverlayBounds? = orbBounds()

    override fun captureBounds(): OverlayBounds? = captureOverlayBounds()?.let { bounds ->
        OverlayBounds(bounds.left, bounds.top, bounds.right, bounds.bottom)
    }

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
                            params.flags = params.flags and
                                WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE.inv()
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
        if (dismissing) return
        val orbWasAttached = orb != null
        val promptWasAttached = powerPrompt != null
        ensureWindows()
        if (powerPromptVisible) ensurePowerPrompt() else detachPowerPrompt()
        val renderedVisual = if (powerPromptVisible) visual.copy(caption = "") else visual
        orb?.render(renderedVisual, currentPlacement())
        val windowSetChanged = !orbWasAttached || promptWasAttached != (powerPrompt != null)
        if (needsOverlayLayoutUpdate(forceLayout, windowSetChanged)) {
            updateLayouts(updateRendererLayout = forceLayout)
        }
        refreshInteractionBounds()
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
        dismissBubble = createDismissBubble(next)
        PhoneControlLog.i(TAG, "overlay_host_changed host=${host.describe()}")
    }

    private fun ensureWindows() {
        if (orb != null) return
        val orbView = PhoneControlOrbView(
            host.context,
            ::onRendererGone,
            ::onRendererRegionChanged,
        )
        val touchView = View(host.context).apply {
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS
            setOnTouchListener(OrbTouchListener())
        }
        val rendererLayout = overlayLayoutParams(
            width = WindowManager.LayoutParams.MATCH_PARENT,
            height = WindowManager.LayoutParams.MATCH_PARENT,
            flags = overlayWebViewWindowFlags(WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE or
                WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL or
                WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN),
        ).apply {
            alpha = host.rendererAlpha
            configureFullDisplayLayout(screenBounds())
        }
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
        PhoneControlLog.i(
            TAG,
            "overlay_attached host=${host.describe()} display_id=${host.displayId}",
        )
    }

    private fun ensurePowerPrompt() {
        if (powerPrompt != null) return
        val prompt = PhoneControlPowerPromptView(
            context = host.context,
            selectedChoice = PhoneControlPowerPreferences.current(context),
            onChoice = ::selectPowerChoice,
            showForgetSgtAdb = SgtAdbCommandBridge.hasPairing(context),
            onForgetSgtAdb = ::forgetSgtAdb,
        )
        val bounds = screenBounds()
        val params = overlayLayoutParams(
            width = minOf(context.dp(304), (bounds.width() - edgeMargin * 2).coerceAtLeast(1)),
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
        onPowerChoiceSelected(choice)
        powerPromptVisible = false
        render()
    }

    private fun forgetSgtAdb() {
        powerPromptVisible = false
        render()
        context.startActivity(PhoneControlActivity.sgtAdbForgetIntent(context))
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

    private fun updateLayouts(
        persistPosition: Boolean = true,
        updateRendererLayout: Boolean = false,
    ) {
        val orbView = orb ?: return
        val rendererLayout = orbParams ?: return
        val touchView = touchTarget ?: return
        val targetLayout = touchParams ?: return
        clampAndSavePosition(persistPosition)
        if (updateRendererLayout) {
            rendererLayout.configureFullDisplayLayout(screenBounds())
            runCatching { host.windowManager.updateViewLayout(orbView, rendererLayout) }
        }
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
        if (visual.visible) {
            interactionBounds = Rect(
                targetLayout.x,
                targetLayout.y,
                targetLayout.x + targetLayout.width,
                targetLayout.y + targetLayout.height,
            ).apply { promptBounds?.let(::union) }
        }
    }

    private fun onRendererRegionChanged(
        source: PhoneControlOrbView,
        region: OverlayBounds?,
    ) {
        if (destroyed || orb !== source) return
        rendererBounds = region?.let { local ->
            val display = screenBounds()
            Rect(
                display.left + local.left,
                display.top + local.top,
                display.left + local.right,
                display.top + local.bottom,
            )
        }
    }

    private fun captureOverlayBounds(): Rect? {
        if (!visual.visible) return null
        val bounds = rendererBounds?.let(::Rect)
        powerPrompt?.let { prompt ->
            powerPromptParams?.let { params ->
                val height = prompt.height.takeIf { it > 0 } ?: context.dp(170)
                val promptBounds = Rect(
                    params.x,
                    params.y,
                    params.x + params.width,
                    params.y + height,
                )
                if (bounds == null) return promptBounds
                bounds.union(promptBounds)
            }
        }
        return bounds ?: interactionBounds?.let(::Rect)
    }

    private fun refreshInteractionBounds() {
        if (!visual.visible) {
            interactionBounds = null
            return
        }
        val targetLayout = touchParams ?: run {
            interactionBounds = null
            return
        }
        val bounds = Rect(
            targetLayout.x,
            targetLayout.y,
            targetLayout.x + targetLayout.width,
            targetLayout.y + targetLayout.height,
        )
        powerPrompt?.let { prompt ->
            powerPromptParams?.let { params ->
                val height = prompt.height.takeIf { it > 0 } ?: context.dp(170)
                bounds.union(params.x, params.y, params.x + params.width, params.y + height)
            }
        }
        interactionBounds = bounds
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
        dismissBubble.hide()
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
        rendererBounds = null
        dismissing = false
    }

    private fun commitOrbDismiss() {
        if (dismissing) return
        dismissing = true
        powerPromptVisible = false
        detachPowerPrompt()
        interactionBounds = null
        touchTarget?.isEnabled = false
        orb?.animateDismiss()
        PhoneControlLog.i(TAG, "orb_dismiss committed=true")
        dismissBubble.swallow(DismissAction.SINGLE, onDismiss)
    }

    private fun detachPowerPrompt() {
        powerPrompt?.let { runCatching { host.windowManager.removeView(it) } }
        powerPrompt = null
        powerPromptParams = null
    }

    private fun createDismissBubble(owner: PhoneControlOverlayWindowHost) =
        DismissBubbleController(
            context = owner.context,
            windowManager = owner.windowManager,
            showDismissAll = false,
            coordinateScaleOverride = 1f,
            windowType = owner.windowType,
            onAttachFailure = {
                PhoneControlLog.e(TAG, "overlay_attach_failed surface=dismiss_target")
            },
        )

    private fun overlayLayoutParams(width: Int, height: Int, flags: Int) =
        WindowManager.LayoutParams(
            width, height, host.windowType, flags, PixelFormat.TRANSLUCENT,
        ).apply { gravity = Gravity.TOP or Gravity.START }

    private fun screenBounds(): Rect =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            host.windowManager.currentWindowMetrics.bounds
        } else legacyScreenBounds()

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
        private val gesture = PhoneControlOrbDragSession(context.dp(5).toFloat())
        private var dragLayoutScheduled = false
        private fun scheduleDragLayout(view: View) {
            if (dragLayoutScheduled) return
            dragLayoutScheduled = true
            view.postOnAnimation {
                dragLayoutScheduled = false
                if (gesture.dragging) updateLayouts(persistPosition = false)
            }
        }

        override fun onTouch(view: View, event: MotionEvent): Boolean {
            val params = touchParams ?: return false
            when (event.actionMasked) {
                MotionEvent.ACTION_DOWN -> {
                    gesture.begin(event.rawX, event.rawY, params.x, params.y)
                    return true
                }
                MotionEvent.ACTION_MOVE -> {
                    gesture.move(event.rawX, event.rawY)?.let { update ->
                        if (update.started) {
                            powerPromptVisible = false
                            detachPowerPrompt()
                            PhoneControlLog.i(TAG, "orb_drag started=true")
                        }
                        params.x = update.windowX
                        params.y = update.windowY
                        scheduleDragLayout(view)
                        dismissBubble.update(
                            dismissBubble.hit(event.rawX, event.rawY, screenBounds()),
                        )
                    }
                    return true
                }
                MotionEvent.ACTION_UP -> {
                    val hit = if (gesture.dragging) {
                        dismissBubble.hit(event.rawX, event.rawY, screenBounds())
                    } else {
                        null
                    }
                    when (gesture.release(hit)) {
                        PhoneControlOrbDragRelease.TAP -> {
                            view.performClick()
                            togglePowerPrompt()
                        }
                        PhoneControlOrbDragRelease.MOVED -> {
                            dismissBubble.hide()
                            clampAndSavePosition()
                            updateLayouts()
                        }
                        PhoneControlOrbDragRelease.DISMISS -> commitOrbDismiss()
                    }
                    return true
                }
                MotionEvent.ACTION_CANCEL -> {
                    gesture.cancel()
                    dismissBubble.hide()
                    return true
                }
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
