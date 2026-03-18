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

    private val rootView = object : FrameLayout(context) {
        override fun dispatchTouchEvent(ev: MotionEvent): Boolean {
            if (spec.touchRegionsOnly && ev.actionMasked != MotionEvent.ACTION_CANCEL && ev.actionMasked != MotionEvent.ACTION_UP) {
                val hit = touchRegions.any { it.contains(ev.x.toInt(), ev.y.toInt()) }
                if (!hit) {
                    return false
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

    private val webView = WebView(context).apply {
        configureOverlayWebView(this, this@PresetOverlayWindow.focusable)
        webChromeClient = object : WebChromeClient() {}
        webViewClient = createOverlayWebViewClient(
            appContext = context.applicationContext,
            logTag = logTag,
            tracker = managedLoadTracker,
            onMessageLog = { Log.d(logTag, it) },
            onMainFrameNavigationFailure = { failure ->
                onNavigationFailureListener?.invoke(failure)
            },
            onPageFinished = { url ->
                pageReady = true
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
        pageReady = false
        managedLoadTracker.begin(prefix = "file:///android_asset/preset_overlay/")
        webView.loadUrl("file:///android_asset/preset_overlay/$assetPage")
    }

    fun loadHtmlContent(
        htmlContent: String,
        baseUrl: String = "file:///android_asset/preset_overlay/",
        clearHistoryAfterLoad: Boolean = false,
    ) {
        Log.d(logTag, "loadHtmlContent baseUrl=$baseUrl len=${htmlContent.length}")
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
