package dev.screengoated.toolbox.mobile.service.preset

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Color
import android.graphics.Outline
import android.graphics.Rect
import android.graphics.drawable.GradientDrawable
import android.util.Log
import android.view.MotionEvent
import android.view.Gravity
import android.view.View
import android.view.ViewOutlineProvider
import android.view.WindowManager
import android.webkit.WebChromeClient
import android.webkit.WebView
import android.widget.FrameLayout
import dev.screengoated.toolbox.mobile.service.OverlayBounds

internal data class PresetOverlayWindowSpec(
    val width: Int,
    val height: Int,
    val x: Int,
    val y: Int,
    val focusable: Boolean,
    val showImeOnFocus: Boolean = false,
    val watchOutsideTouch: Boolean = false,
    val assetPage: String? = null,
    val htmlContent: String? = null,
    val baseUrl: String = "file:///android_asset/preset_overlay/",
    val clipToOutline: Boolean = true,
    val touchRegionsOnly: Boolean = false,
)

internal data class OverlayNavigationFailure(
    val url: String?,
    val description: String,
)

private sealed interface OverlayRecoverySource {
    data class Asset(val assetPage: String) : OverlayRecoverySource
    data class Html(val htmlContent: String, val baseUrl: String) : OverlayRecoverySource
}

@SuppressLint("SetJavaScriptEnabled")
internal class PresetOverlayWindow(
    context: Context,
    private val windowManager: WindowManager,
    spec: PresetOverlayWindowSpec,
    private val onMessage: (String) -> Unit,
    private val onBoundsChanged: (OverlayBounds) -> Unit = {},
    private val onOutsideTouch: () -> Unit = {},
) {
    private val webViewContext = context
    private val appContext = context.applicationContext
    private val logTag: String = "PresetOverlay"
    private val supportsFocus = spec.focusable
    private var currentFocusable = spec.focusable
    private val showImeOnFocus = spec.showImeOnFocus
    private val watchOutsideTouch = spec.watchOutsideTouch
    private val cornerRadiusPx = context.resources.displayMetrics.density * 18f
    private val managedLoadTracker = OverlayManagedLoadTracker()
    private val layoutParams = WindowManager.LayoutParams().apply {
        copyFrom(
            WindowManager.LayoutParams(
                spec.width,
                spec.height,
                WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
                buildOverlayWindowFlags(currentFocusable, watchOutsideTouch),
                android.graphics.PixelFormat.TRANSLUCENT,
            ),
        )
        gravity = Gravity.TOP or Gravity.START
        x = spec.x
        y = spec.y
        if (currentFocusable && showImeOnFocus) {
            softInputMode = WindowManager.LayoutParams.SOFT_INPUT_ADJUST_PAN or
                WindowManager.LayoutParams.SOFT_INPUT_STATE_VISIBLE
        }
    }
    private val touchRegions = mutableListOf<Rect>()

    private var touchAccepted = false
    private val rootView = object : FrameLayout(context) {
        override fun dispatchTouchEvent(ev: MotionEvent): Boolean {
            if (ev.actionMasked == MotionEvent.ACTION_OUTSIDE) {
                onOutsideTouch()
                return false
            }
            if (spec.touchRegionsOnly) {
                when (ev.actionMasked) {
                    MotionEvent.ACTION_DOWN -> {
                        val hit = touchRegions.any { it.contains(ev.x.toInt(), ev.y.toInt()) }
                        touchAccepted = hit
                        if (!hit) return false
                    }
                    MotionEvent.ACTION_CANCEL, MotionEvent.ACTION_UP -> {
                        touchAccepted = false
                    }
                    else -> {
                        if (!touchAccepted) return false
                    }
                }
            }
            return super.dispatchTouchEvent(ev)
        }
    }.apply {
        setBackgroundColor(Color.TRANSPARENT)
        clipToOutline = spec.clipToOutline
        clipChildren = spec.clipToOutline
        background = GradientDrawable().apply {
            setColor(Color.TRANSPARENT)
            cornerRadius = cornerRadiusPx
        }
        outlineProvider = object : ViewOutlineProvider() {
            override fun getOutline(view: View, outline: Outline) {
                outline.setRoundRect(0, 0, view.width, view.height, cornerRadiusPx)
            }
        }
    }
    private val pendingScripts = mutableListOf<String>()
    private var pageReady = false
    private var attached = false
    private var layoutApplyScheduled = false
    private var onPageFinishedListener: ((String?) -> Unit)? = null
    private var onNavigationFailureListener: ((OverlayNavigationFailure) -> Unit)? = null
    private var recoverySource: OverlayRecoverySource? = null
    private var recoveryAttemptInFlight = false

    private var webView = createManagedWebView()
    private var inputFocusCoordinator = createInputFocusCoordinator()

    init {
        rootView.addView(
            webView,
            matchParentLayoutParams(),
        )
        when {
            spec.htmlContent != null -> loadHtmlContent(spec.htmlContent, spec.baseUrl)
            spec.assetPage != null -> loadAssetPage(spec.assetPage)
        }
    }

    fun show() {
        if (attached) {
            if (currentFocusable) {
                inputFocusCoordinator.requestInitialFocus()
            }
            return
        }
        windowManager.addView(rootView, layoutParams)
        attached = true
        if (currentFocusable) {
            inputFocusCoordinator.requestInitialFocus()
        }
    }

    fun bringToFront(refocusIme: Boolean = false) {
        if (!attached) {
            return
        }
        runCatching { windowManager.removeViewImmediate(rootView) }
        attached = false
        windowManager.addView(rootView, layoutParams)
        attached = true
        if (currentFocusable) {
            inputFocusCoordinator.refocus(refocusIme)
        }
    }

    fun hide() {
        if (!attached) {
            return
        }
        inputFocusCoordinator.hideKeyboard()
        runCatching { windowManager.removeView(rootView) }
        attached = false
    }

    /**
     * Visually suppress/unsuppress the window without removing from WindowManager.
     * Avoids the removeView/addView cycle that causes all overlays to blink.
     */
    fun setSuppressed(suppressed: Boolean) {
        rootView.visibility = if (suppressed) View.INVISIBLE else View.VISIBLE
    }

    fun destroy() {
        hide()
        webView.destroy()
    }

    fun updateBounds(bounds: OverlayBounds) {
        layoutParams.x = bounds.x
        layoutParams.y = bounds.y
        layoutParams.width = bounds.width
        layoutParams.height = bounds.height
        if (attached) {
            scheduleLayoutApply()
        } else {
            onBoundsChanged(currentBounds())
        }
    }

    fun moveBy(
        dx: Int,
        dy: Int,
        screenBounds: Rect,
    ) {
        val current = currentBounds()
        updateBounds(
            current.copy(
                x = (current.x + dx).coerceIn(0, (screenBounds.width() - current.width).coerceAtLeast(0)),
                y = (current.y + dy).coerceIn(0, (screenBounds.height() - current.height).coerceAtLeast(0)),
            ),
        )
    }

    fun currentBounds(): OverlayBounds {
        return OverlayBounds(
            x = layoutParams.x,
            y = layoutParams.y,
            width = layoutParams.width,
            height = layoutParams.height,
        )
    }

    fun loadAssetPage(assetPage: String) {
        recoverySource = OverlayRecoverySource.Asset(assetPage)
        pageReady = false
        managedLoadTracker.begin(prefix = "file:///android_asset/preset_overlay/")
        webView.loadUrl("file:///android_asset/preset_overlay/$assetPage")
    }

    fun loadHtmlContent(
        htmlContent: String,
        baseUrl: String = "file:///android_asset/preset_overlay/",
        clearHistoryAfterLoad: Boolean = false,
        rememberForRecovery: Boolean = true,
    ) {
        Log.d(logTag, "loadHtmlContent baseUrl=$baseUrl len=${htmlContent.length}")
        if (rememberForRecovery) {
            recoverySource = OverlayRecoverySource.Html(
                htmlContent = htmlContent,
                baseUrl = baseUrl,
            )
        }
        pageReady = false
        managedLoadTracker.begin(prefix = baseUrl, clearHistoryAfterLoad = clearHistoryAfterLoad)
        webView.loadDataWithBaseURL(
            baseUrl,
            htmlContent,
            "text/html",
            "utf-8",
            null,
        )
    }

    fun stopLoading() {
        runCatching { webView.stopLoading() }
    }

    fun runScript(script: String) {
        if (pageReady) {
            webView.evaluateJavascript(script, null)
        } else {
            pendingScripts += script
        }
    }

    fun runScriptForResult(
        script: String,
        onResult: (String?) -> Unit,
    ) {
        if (pageReady) {
            webView.evaluateJavascript(script, onResult)
        } else {
            rootView.post {
                if (pageReady) {
                    webView.evaluateJavascript(script, onResult)
                } else {
                    onResult(null)
                }
            }
        }
    }

    fun setFocusable(focusable: Boolean) {
        if (focusable && !supportsFocus) {
            return
        }
        if (focusable == currentFocusable) {
            return
        }
        currentFocusable = focusable
        layoutParams.flags = buildOverlayWindowFlags(focusable, watchOutsideTouch = watchOutsideTouch)
        if (focusable) {
            layoutParams.softInputMode = WindowManager.LayoutParams.SOFT_INPUT_ADJUST_PAN or
                WindowManager.LayoutParams.SOFT_INPUT_STATE_VISIBLE
        } else {
            layoutParams.softInputMode = 0
        }
        if (attached) {
            runCatching { windowManager.removeViewImmediate(rootView) }
            attached = false
            windowManager.addView(rootView, layoutParams)
            attached = true
        }
        if (focusable) {
            webView.isFocusable = true
            webView.isFocusableInTouchMode = true
            webView.post {
                webView.requestFocusFromTouch()
                webView.requestFocus()
                val imm = webView.context.getSystemService(android.view.inputmethod.InputMethodManager::class.java)
                imm?.showSoftInput(webView, android.view.inputmethod.InputMethodManager.SHOW_IMPLICIT)
            }
            webView.postDelayed({
                webView.requestFocusFromTouch()
                val imm = webView.context.getSystemService(android.view.inputmethod.InputMethodManager::class.java)
                imm?.showSoftInput(webView, android.view.inputmethod.InputMethodManager.SHOW_IMPLICIT)
            }, 200L)
        } else {
            webView.clearFocus()
            rootView.clearFocus()
            webView.isFocusable = false
            webView.isFocusableInTouchMode = false
            val imm = webView.context.getSystemService(android.view.inputmethod.InputMethodManager::class.java)
            imm?.hideSoftInputFromWindow(webView.windowToken, 0)
        }
    }

    fun setWindowAlpha(alpha: Float) {
        // Apply opacity on the View layer, NOT layoutParams.alpha.
        // Android's tapjacking protection blocks touch on windows with
        // layoutParams.alpha < ~0.5, making the overlay uninteractable.
        rootView.alpha = alpha.coerceIn(0.1f, 1.0f)
    }

    fun updateTouchRegions(regions: List<Rect>) {
        touchRegions.clear()
        touchRegions.addAll(regions)
    }

    fun setOnPageFinishedListener(listener: ((String?) -> Unit)?) {
        onPageFinishedListener = listener
    }

    fun setOnNavigationFailureListener(listener: ((OverlayNavigationFailure) -> Unit)?) {
        onNavigationFailureListener = listener
    }

    fun currentUrl(): String? = webView.url

    fun goBack() {
        Log.d(logTag, "goBack canGoBack=${webView.canGoBack()} url=${webView.url}")
        if (webView.canGoBack()) {
            webView.goBack()
        }
    }

    fun goForward() {
        Log.d(logTag, "goForward canGoForward=${webView.canGoForward()} url=${webView.url}")
        if (webView.canGoForward()) {
            webView.goForward()
        }
    }

    fun goBackInPageHistory() {
        Log.d(logTag, "goBackInPageHistory url=${webView.url}")
        runScript("history.back();")
    }

    fun goForwardInPageHistory() {
        Log.d(logTag, "goForwardInPageHistory url=${webView.url}")
        runScript("history.forward();")
    }

    fun historyState(): OverlayHistoryState {
        val list = runCatching { webView.copyBackForwardList() }.getOrNull()
        val currentItem = list?.currentItem
        return OverlayHistoryState(
            currentIndex = list?.currentIndex ?: 0,
            lastIndex = ((list?.size ?: 1) - 1).coerceAtLeast(0),
            currentUrl = currentItem?.url ?: webView.url,
        )
    }

    private fun flushPendingScripts() {
        if (!pageReady) {
            return
        }
        val scripts = pendingScripts.toList()
        pendingScripts.clear()
        scripts.forEach { script ->
            webView.evaluateJavascript(script, null)
        }
    }

    private fun recoverFromNavigationFailure(failure: OverlayNavigationFailure) {
        val source = recoverySource
        if (source == null) {
            loadRecoveryFailureHtml(failure)
            return
        }
        if (recoveryAttemptInFlight) {
            loadRecoveryFailureHtml(failure)
            return
        }
        Log.w(logTag, "recoverFromNavigationFailure url=${failure.url} desc=${failure.description}")
        recoveryAttemptInFlight = true
        pageReady = false
        pendingScripts.clear()
        runCatching { webView.stopLoading() }
        when (source) {
            is OverlayRecoverySource.Asset -> {
                managedLoadTracker.begin(
                    prefix = "file:///android_asset/preset_overlay/",
                    clearHistoryAfterLoad = true,
                )
                webView.loadUrl("file:///android_asset/preset_overlay/${source.assetPage}")
            }
            is OverlayRecoverySource.Html -> {
                loadHtmlContent(
                    htmlContent = source.htmlContent,
                    baseUrl = source.baseUrl,
                    clearHistoryAfterLoad = true,
                    rememberForRecovery = false,
                )
            }
        }
    }

    private fun loadRecoveryFailureHtml(failure: OverlayNavigationFailure) {
        recoveryAttemptInFlight = false
        pageReady = false
        pendingScripts.clear()
        loadHtmlContent(
            htmlContent = overlayRecoveryFailureHtml(failure.description),
            baseUrl = "file:///android_asset/preset_overlay/",
            clearHistoryAfterLoad = true,
            rememberForRecovery = false,
        )
    }

    private fun createManagedWebView(): WebView {
        return WebView(webViewContext).apply {
            configureOverlayWebView(this, supportsFocus)
            webChromeClient = object : WebChromeClient() {}
            webViewClient = createOverlayWebViewClient(
                appContext = appContext,
                logTag = logTag,
                tracker = managedLoadTracker,
                onMessageLog = { Log.d(logTag, it) },
                onRenderProcessGone = { deadView, detail ->
                    recoverFromRenderProcessGone(deadView, detail)
                },
                onMainFrameNavigationFailure = { failure ->
                    recoverFromNavigationFailure(failure)
                    onNavigationFailureListener?.invoke(failure)
                },
                onPageFinished = { url ->
                    pageReady = true
                    recoveryAttemptInFlight = false
                    flushPendingScripts()
                    onPageFinishedListener?.invoke(url)
                },
            )
            addJavascriptInterface(
                object {
                    @android.webkit.JavascriptInterface
                    fun postMessage(message: String) {
                        post {
                            if (!handleOverlayHostMessage(appContext, message, logTag)) {
                                onMessage(message)
                            }
                        }
                    }
                },
                "sgtAndroid",
            )
        }
    }

    private fun createInputFocusCoordinator(): OverlayInputFocusCoordinator {
        return OverlayInputFocusCoordinator(
            context = webViewContext,
            webView = webView,
            focusable = supportsFocus,
            showImeOnFocus = showImeOnFocus,
        )
    }

    private fun recoverFromRenderProcessGone(
        deadView: WebView?,
        detail: android.webkit.RenderProcessGoneDetail?,
    ): Boolean {
        val source = recoverySource
        pageReady = false
        recoveryAttemptInFlight = false
        pendingScripts.clear()
        Log.w(
            logTag,
            "recoverFromRenderProcessGone crashed=${detail?.didCrash() ?: "null"} source=${source != null}",
        )
        val oldView = deadView ?: webView
        runCatching { rootView.removeView(oldView) }
        if (oldView === webView) {
            webView = createManagedWebView()
            inputFocusCoordinator = createInputFocusCoordinator()
            rootView.addView(webView, matchParentLayoutParams())
            if (currentFocusable && attached) {
                inputFocusCoordinator.requestInitialFocus()
            }
            when (source) {
                is OverlayRecoverySource.Asset -> loadAssetPage(source.assetPage)
                is OverlayRecoverySource.Html -> loadHtmlContent(source.htmlContent, source.baseUrl)
                null -> loadRecoveryFailureHtml(
                    OverlayNavigationFailure(
                        url = null,
                        description = "renderer_gone",
                    ),
                )
            }
        }
        runCatching { oldView.destroy() }
        return true
    }

    private fun matchParentLayoutParams(): FrameLayout.LayoutParams {
        return FrameLayout.LayoutParams(
            FrameLayout.LayoutParams.MATCH_PARENT,
            FrameLayout.LayoutParams.MATCH_PARENT,
        )
    }

    private fun scheduleLayoutApply() {
        if (layoutApplyScheduled) {
            return
        }
        layoutApplyScheduled = true
        rootView.postOnAnimation {
            layoutApplyScheduled = false
            if (attached) {
                runCatching { windowManager.updateViewLayout(rootView, layoutParams) }
            }
            onBoundsChanged(currentBounds())
        }
    }
}

internal data class OverlayHistoryState(
    val currentIndex: Int,
    val lastIndex: Int,
    val currentUrl: String?,
)
