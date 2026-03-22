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
) {
    private val logTag: String = "PresetOverlay"
    private val focusable = spec.focusable
    private val cornerRadiusPx = context.resources.displayMetrics.density * 18f
    private val managedLoadTracker = OverlayManagedLoadTracker()
    private val layoutParams = WindowManager.LayoutParams().apply {
        copyFrom(
            WindowManager.LayoutParams(
                spec.width,
                spec.height,
                WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
                buildOverlayWindowFlags(spec.focusable),
                android.graphics.PixelFormat.TRANSLUCENT,
            ),
        )
        gravity = Gravity.TOP or Gravity.START
        x = spec.x
        y = spec.y
        if (focusable && spec.showImeOnFocus) {
            softInputMode = WindowManager.LayoutParams.SOFT_INPUT_ADJUST_PAN or
                WindowManager.LayoutParams.SOFT_INPUT_STATE_VISIBLE
        }
    }
    private val touchRegions = mutableListOf<Rect>()

    private var touchAccepted = false
    private val rootView = object : FrameLayout(context) {
        override fun dispatchTouchEvent(ev: MotionEvent): Boolean {
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

    private val webView = WebView(context).apply {
        configureOverlayWebView(this, this@PresetOverlayWindow.focusable)
        webChromeClient = object : WebChromeClient() {}
        webViewClient = createOverlayWebViewClient(
            appContext = context.applicationContext,
            logTag = logTag,
            tracker = managedLoadTracker,
            onMessageLog = { Log.d(logTag, it) },
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
                    post { onMessage(message) }
                }
            },
            "sgtAndroid",
        )
    }
    private val inputFocusCoordinator = OverlayInputFocusCoordinator(
        context = context,
        webView = webView,
        focusable = focusable,
        showImeOnFocus = spec.showImeOnFocus,
    )

    init {
        rootView.addView(
            webView,
            FrameLayout.LayoutParams(
                FrameLayout.LayoutParams.MATCH_PARENT,
                FrameLayout.LayoutParams.MATCH_PARENT,
            ),
        )
        when {
            spec.htmlContent != null -> loadHtmlContent(spec.htmlContent, spec.baseUrl)
            spec.assetPage != null -> loadAssetPage(spec.assetPage)
        }
    }

    fun show() {
        if (attached) {
            if (focusable) {
                inputFocusCoordinator.requestInitialFocus()
            }
            return
        }
        windowManager.addView(rootView, layoutParams)
        attached = true
        if (focusable) {
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
        if (focusable) {
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
        layoutParams.flags = buildOverlayWindowFlags(focusable)
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

private fun overlayRecoveryFailureHtml(description: String): String {
    val escaped = escapeHtml(description)
    return """
        <!DOCTYPE html>
        <html>
        <head>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <style>
                body {
                    margin: 0;
                    min-height: 100vh;
                    display: flex;
                    align-items: center;
                    justify-content: center;
                    background: linear-gradient(180deg, #101218, #171b24);
                    color: #f5f7fb;
                    font-family: 'Google Sans Flex', 'Segoe UI', system-ui, sans-serif;
                }
                .card {
                    max-width: 420px;
                    margin: 20px;
                    padding: 18px 20px;
                    border-radius: 18px;
                    background: rgba(27, 31, 43, 0.96);
                    border: 1px solid rgba(255, 255, 255, 0.1);
                    box-shadow: 0 18px 48px rgba(0, 0, 0, 0.28);
                }
                h1 {
                    margin: 0 0 8px;
                    font-size: 18px;
                    font-weight: 700;
                }
                p {
                    margin: 0;
                    line-height: 1.45;
                    color: #d7deeb;
                    font-size: 14px;
                }
                .meta {
                    margin-top: 10px;
                    font-size: 12px;
                    color: #99a6bd;
                }
            </style>
        </head>
        <body>
            <div class="card">
                <h1>Overlay recovered</h1>
                <p>The page hit a loading error, so the overlay restored a safe view instead of leaving the Android error page onscreen.</p>
                <div class="meta">$escaped</div>
            </div>
        </body>
        </html>
    """.trimIndent()
}
