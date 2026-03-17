package dev.screengoated.toolbox.mobile.service.preset

import android.annotation.SuppressLint
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.graphics.Color
import android.graphics.Outline
import android.graphics.Rect
import android.graphics.drawable.GradientDrawable
import android.net.Uri
import android.util.Log
import android.view.MotionEvent
import android.view.Gravity
import android.view.View
import android.view.ViewOutlineProvider
import android.view.WindowManager
import android.view.inputmethod.InputMethodManager
import android.webkit.WebChromeClient
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebViewRenderProcess
import android.webkit.WebView
import android.webkit.WebViewClient
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
    private val appContext = context.applicationContext
    private val focusable = spec.focusable
    private val cornerRadiusPx = context.resources.displayMetrics.density * 18f
    private val layoutParams = WindowManager.LayoutParams().apply {
        copyFrom(
            WindowManager.LayoutParams(
                spec.width,
                spec.height,
                WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
                buildFlags(spec.focusable),
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
    private val showImeOnFocus = spec.showImeOnFocus

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
    private var clearHistoryOnNextPageFinished = false
    private var managedLoadInFlight = false
    private var managedLoadPrefix: String? = null

    private val webView = WebView(context).apply {
        overScrollMode = WebView.OVER_SCROLL_NEVER
        setBackgroundColor(Color.TRANSPARENT)
        isVerticalScrollBarEnabled = false
        isHorizontalScrollBarEnabled = false
        isFocusable = this@PresetOverlayWindow.focusable
        isFocusableInTouchMode = this@PresetOverlayWindow.focusable
        settings.javaScriptEnabled = true
        settings.domStorageEnabled = true
        settings.allowFileAccess = true
        settings.mediaPlaybackRequiresUserGesture = false
        settings.builtInZoomControls = false
        settings.displayZoomControls = false
        settings.setSupportZoom(false)
        setLayerType(View.LAYER_TYPE_HARDWARE, null)
        webChromeClient = object : WebChromeClient() {}
        webViewClient = object : WebViewClient() {
            override fun shouldOverrideUrlLoading(
                view: WebView?,
                request: WebResourceRequest?,
            ): Boolean {
                val url = request?.url?.toString().orEmpty()
                if (url.isBlank()) {
                    return false
                }
                if (isManagedWebViewUrl(url)) {
                    return false
                }
                if (!url.startsWith("http://") && !url.startsWith("https://")) {
                    val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url)).apply {
                        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                    }
                    return try {
                        appContext.startActivity(intent)
                        true
                    } catch (_: ActivityNotFoundException) {
                        Log.w(logTag, "No activity found for external url=$url")
                        true
                    }
                }
                return false
            }

            override fun onPageStarted(
                view: WebView?,
                url: String?,
                favicon: android.graphics.Bitmap?,
            ) {
                Log.d(logTag, "pageStarted url=${url ?: "null"}")
            }

            override fun onPageFinished(
                view: WebView?,
                url: String?,
            ) {
                Log.d(logTag, "pageFinished url=${url ?: "null"}")
                if (managedLoadInFlight && !matchesManagedLoad(url)) {
                    Log.d(logTag, "pageFinished ignored stale url=${url ?: "null"} managedPrefix=${managedLoadPrefix ?: "null"}")
                    return
                }
                managedLoadInFlight = false
                managedLoadPrefix = null
                if (clearHistoryOnNextPageFinished) {
                    clearHistoryOnNextPageFinished = false
                    runCatching { view?.clearHistory() }
                }
                pageReady = true
                flushPendingScripts()
                onPageFinishedListener?.invoke(url)
            }

            override fun onReceivedError(
                view: WebView?,
                request: WebResourceRequest?,
                error: WebResourceError?,
            ) {
                Log.e(
                    logTag,
                    "receivedError url=${request?.url ?: "null"}" +
                        " code=${error?.errorCode ?: "null"}" +
                        " desc=${error?.description ?: "null"}",
                )
                if (request?.isForMainFrame == true) {
                    onNavigationFailureListener?.invoke(
                        OverlayNavigationFailure(
                            url = request.url?.toString(),
                            description = "network:${error?.errorCode}:${error?.description}",
                        ),
                    )
                }
            }

            override fun onReceivedHttpError(
                view: WebView?,
                request: WebResourceRequest?,
                errorResponse: WebResourceResponse?,
            ) {
                Log.e(
                    logTag,
                    "receivedHttpError url=${request?.url ?: "null"}" +
                        " status=${errorResponse?.statusCode ?: "null"}" +
                        " reason=${errorResponse?.reasonPhrase ?: "null"}",
                )
                if (request?.isForMainFrame == true && (errorResponse?.statusCode ?: 0) >= 400) {
                    onNavigationFailureListener?.invoke(
                        OverlayNavigationFailure(
                            url = request.url?.toString(),
                            description = "http:${errorResponse?.statusCode}:${errorResponse?.reasonPhrase.orEmpty()}",
                        ),
                    )
                }
            }

            override fun onRenderProcessGone(
                view: WebView?,
                detail: android.webkit.RenderProcessGoneDetail?,
            ): Boolean {
                Log.e(
                    logTag,
                    "renderProcessGone crashed=${detail?.didCrash() ?: "null"}" +
                        " priority=${detail?.rendererPriorityAtExit() ?: "null"}",
                )
                return false
            }
        }
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
                requestInputFocus()
            }
            return
        }
        windowManager.addView(rootView, layoutParams)
        attached = true
        if (focusable) {
            requestInputFocus()
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
            if (refocusIme) {
                requestInputFocus()
            } else {
                focusWebView(showKeyboard = false)
            }
        }
    }

    fun hide() {
        if (!attached) {
            return
        }
        hideKeyboard()
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
        managedLoadInFlight = true
        managedLoadPrefix = "file:///android_asset/preset_overlay/"
        webView.loadUrl("file:///android_asset/preset_overlay/$assetPage")
    }

    fun loadHtmlContent(
        htmlContent: String,
        baseUrl: String = "file:///android_asset/preset_overlay/",
        clearHistoryAfterLoad: Boolean = false,
    ) {
        Log.d(logTag, "loadHtmlContent baseUrl=$baseUrl len=${htmlContent.length}")
        pageReady = false
        clearHistoryOnNextPageFinished = clearHistoryAfterLoad
        managedLoadInFlight = true
        managedLoadPrefix = baseUrl
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

    private fun requestInputFocus() {
        focusWebView(showKeyboard = showImeOnFocus)
        if (showImeOnFocus) {
            webView.postDelayed({ focusWebView(showKeyboard = true) }, 120L)
            webView.postDelayed({ focusWebView(showKeyboard = true) }, 260L)
        }
    }

    private fun focusWebView(showKeyboard: Boolean) {
        webView.post {
            webView.requestFocusFromTouch()
            webView.requestFocus()
            if (showKeyboard) {
                webView.evaluateJavascript(
                    "window.focusEditor && window.focusEditor();",
                    null,
                )
                val inputMethodManager = appContext.getSystemService(InputMethodManager::class.java)
                inputMethodManager?.showSoftInput(webView, InputMethodManager.SHOW_IMPLICIT)
            }
        }
    }

    private fun matchesManagedLoad(url: String?): Boolean {
        val prefix = managedLoadPrefix ?: return true
        val currentUrl = url ?: return false
        return currentUrl.startsWith(prefix)
    }

    private fun isManagedWebViewUrl(url: String): Boolean {
        return url.startsWith("http://") ||
            url.startsWith("https://") ||
            url.startsWith("file:///android_asset/") ||
            url.startsWith("about:")
    }

    private fun hideKeyboard() {
        if (!focusable) {
            return
        }
        val inputMethodManager = appContext.getSystemService(InputMethodManager::class.java)
        inputMethodManager?.hideSoftInputFromWindow(webView.windowToken, 0)
    }

    private companion object {
        fun buildFlags(focusable: Boolean): Int {
            var flags = WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
                WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL
            if (!focusable) {
                flags = flags or WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE
            }
            return flags
        }
    }
}

internal data class OverlayHistoryState(
    val currentIndex: Int,
    val lastIndex: Int,
    val currentUrl: String?,
)
