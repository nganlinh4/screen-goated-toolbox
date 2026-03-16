package dev.screengoated.toolbox.mobile.service.preset

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Color
import android.graphics.Outline
import android.graphics.Rect
import android.graphics.drawable.GradientDrawable
import android.view.MotionEvent
import android.view.Gravity
import android.view.View
import android.view.ViewOutlineProvider
import android.view.WindowManager
import android.view.inputmethod.InputMethodManager
import android.webkit.WebChromeClient
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
    val assetPage: String? = null,
    val htmlContent: String? = null,
    val baseUrl: String = "file:///android_asset/preset_overlay/",
    val clipToOutline: Boolean = true,
    val touchRegionsOnly: Boolean = false,
)

@SuppressLint("SetJavaScriptEnabled")
internal class PresetOverlayWindow(
    context: Context,
    private val windowManager: WindowManager,
    spec: PresetOverlayWindowSpec,
    private val onMessage: (String) -> Unit,
    private val onBoundsChanged: (OverlayBounds) -> Unit = {},
) {
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
        if (focusable) {
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
            override fun onPageFinished(
                view: WebView?,
                url: String?,
            ) {
                pageReady = true
                flushPendingScripts()
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
        webView.loadUrl("file:///android_asset/preset_overlay/$assetPage")
    }

    fun loadHtmlContent(
        htmlContent: String,
        baseUrl: String = "file:///android_asset/preset_overlay/",
    ) {
        pageReady = false
        webView.loadDataWithBaseURL(
            baseUrl,
            htmlContent,
            "text/html",
            "utf-8",
            null,
        )
    }

    fun runScript(script: String) {
        if (pageReady) {
            webView.evaluateJavascript(script, null)
        } else {
            pendingScripts += script
        }
    }

    fun updateTouchRegions(regions: List<Rect>) {
        touchRegions.clear()
        touchRegions.addAll(regions)
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
        webView.post {
            webView.requestFocusFromTouch()
            webView.requestFocus()
            val inputMethodManager = appContext.getSystemService(InputMethodManager::class.java)
            inputMethodManager?.showSoftInput(webView, InputMethodManager.SHOW_IMPLICIT)
        }
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
