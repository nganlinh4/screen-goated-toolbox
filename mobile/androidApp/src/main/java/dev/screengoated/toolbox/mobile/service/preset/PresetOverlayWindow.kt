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
import dev.screengoated.toolbox.mobile.service.overlay.overlayWebViewWindowFlags
import dev.screengoated.toolbox.mobile.preset.INPUT_AUDIO_PREVIEW_PLACEHOLDER

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
    private val resultWindowId = spec.resultWindowId
    private val resultIsDark = spec.resultIsDark
    private val resultCapabilityNotice = ResultCapabilityNotice(context, spec.resultCapabilityMessage)
    private val resultScripts = TrustedResultScripts()
    private val resultAssetPolicy = if (resultWindowId != null) ResultAssetPolicy(context) else null
    private var untrustedResult = false
    private var trustedResultBridgeAvailable = false
    private var resultHistoryOffset = 0
    private var currentFocusable = spec.focusable
    private val showImeOnFocus = spec.showImeOnFocus
    private val watchOutsideTouch = spec.watchOutsideTouch
    private val cornerRadiusPx = context.resources.displayMetrics.density * 18f
    private var managedLoadTracker = OverlayManagedLoadTracker()
    private val layoutParams = WindowManager.LayoutParams().apply {
        copyFrom(
            WindowManager.LayoutParams(
                spec.width,
                spec.height,
                WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
                overlayWebViewWindowFlags(
                    buildOverlayWindowFlags(currentFocusable, watchOutsideTouch),
                ),
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
            if (ev.actionMasked == MotionEvent.ACTION_DOWN && resultWindowId != null &&
                (untrustedResult || !trustedResultBridgeAvailable || !isTrustedResultUrl(webView.url))
            ) {
                onMessage(org.json.JSONObject().put("type", "activateResultWindow").put("windowId", resultWindowId).toString())
            }
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
        resultAssetPolicy?.audioPreview?.replace(null)
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

    fun currentBounds(): OverlayBounds = OverlayBounds(
        x = layoutParams.x, y = layoutParams.y,
        width = layoutParams.width, height = layoutParams.height,
    )

    fun loadAssetPage(assetPage: String) {
        resultAssetPolicy?.audioPreview?.replace(null)
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
        untrustedContent: Boolean = false,
        audioPreview: ByteArray? = null,
    ) {
        if (needsFreshResultView(resultWindowId != null, untrustedResult, untrustedContent)) {
            replaceResultView(untrustedContent, 0)
        }
        val actualBaseUrl = if (resultWindowId != null && !untrustedContent) RESULT_TRUSTED_URL else baseUrl
        if (rememberForRecovery) {
            recoverySource = OverlayRecoverySource.Html(
                htmlContent = htmlContent,
                baseUrl = baseUrl,
                untrusted = untrustedContent,
                audioPreview = audioPreview,
            )
        }
        pageReady = false
        managedLoadTracker.begin(prefix = actualBaseUrl, clearHistoryAfterLoad = clearHistoryAfterLoad)
        webView.loadDataWithBaseURL(
            actualBaseUrl,
            (if (resultWindowId != null && !untrustedContent) resultScripts.prepare(htmlContent) else htmlContent)
                .replace(INPUT_AUDIO_PREVIEW_PLACEHOLDER, resultAssetPolicy?.audioPreview?.replace(audioPreview) ?: INPUT_AUDIO_PREVIEW_PLACEHOLDER),
            "text/html",
            "utf-8",
            null,
        )
    }
    fun stopLoading() {
        runCatching { webView.stopLoading() }
    }
    fun runScript(script: String) {
        if (resultWindowId != null && untrustedResult) return
        if (pageReady) {
            if (resultWindowId == null || isTrustedResultUrl(webView.url)) webView.evaluateJavascript(if (resultWindowId == null) script else resultScripts.wrap(script), null)
        } else {
            pendingScripts += script
        }
    }

    fun runScriptForResult(
        script: String,
        onResult: (String?) -> Unit,
    ) {
        if (resultWindowId != null && (untrustedResult || !isTrustedResultUrl(webView.url))) {
            onResult(null)
            return
        }
        if (pageReady) {
            webView.evaluateJavascript(if (resultWindowId == null) script else resultScripts.wrap(script), onResult)
        } else {
            rootView.post {
                if (pageReady && (resultWindowId == null || (!untrustedResult && isTrustedResultUrl(webView.url)))) {
                    webView.evaluateJavascript(if (resultWindowId == null) script else resultScripts.wrap(script), onResult)
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
        layoutParams.flags = overlayWebViewWindowFlags(
            buildOverlayWindowFlags(focusable, watchOutsideTouch = watchOutsideTouch),
        )
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
        if (webView.canGoBack()) {
            webView.goBack()
        }
    }

    fun goForward() {
        if (webView.canGoForward()) {
            webView.goForward()
        }
    }

    fun historyState(): OverlayHistoryState {
        val list = runCatching { webView.copyBackForwardList() }.getOrNull()
        val currentItem = list?.currentItem
        return OverlayHistoryState(
            currentIndex = (list?.currentIndex ?: 0) + resultHistoryOffset,
            lastIndex = ((list?.size ?: 1) - 1).coerceAtLeast(0) + resultHistoryOffset,
            currentUrl = currentItem?.url ?: webView.url,
        )
    }

    private fun flushPendingScripts() {
        if (!pageReady) {
            return
        }
        val scripts = pendingScripts.toList()
        pendingScripts.clear()
        if (resultWindowId != null && (untrustedResult || !isTrustedResultUrl(webView.url))) return
        scripts.forEach { script ->
            webView.evaluateJavascript(if (resultWindowId == null) script else resultScripts.wrap(script), null)
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
                    untrustedContent = source.untrusted,
                    audioPreview = source.audioPreview,
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
        val untrustedView = untrustedResult
        val viewTracker = managedLoadTracker
        val gestureGate = ResultDocumentGestureGate()
        return WebView(webViewContext).apply {
            val targetView = this
            configureOverlayWebView(this, supportsFocus)
            if (untrustedView) settings.allowFileAccess = false
            webChromeClient = object : WebChromeClient() {}
            webViewClient = createOverlayWebViewClient(
                appContext = appContext,
                logTag = logTag,
                tracker = viewTracker,
                onMessageLog = { Log.d(logTag, it) },
                onRenderProcessGone = { deadView, detail ->
                    recoverFromRenderProcessGone(deadView, detail)
                },
                onMainFrameNavigationFailure = { failure ->
                    if (targetView === webView) {
                        recoverFromNavigationFailure(failure)
                        onNavigationFailureListener?.invoke(failure)
                    }
                },
                onPageFinished = { url ->
                    if (targetView === webView) {
                        pageReady = true
                        recoveryAttemptInFlight = false
                        flushPendingScripts()
                        onPageFinishedListener?.invoke(url)
                    }
                },
                onMainFrameNavigation = { source, request ->
                    revokePreviewOnNavigation(source === webView, resultAssetPolicy?.audioPreview)
                    val url = request.url.toString()
                    if (resultWindowId != null && !untrustedView &&
                        (request.url.scheme == "http" || request.url.scheme == "https") &&
                        !isTrustedResultUrl(url)
                    ) {
                        rootView.post {
                            if (source === webView) {
                                replaceResultView(true, 1)
                                webView.loadUrl(url)
                            }
                        }
                        true
                    } else false
                },
                interceptRequest = { request -> resultAssetPolicy?.intercept(request, !untrustedView) },
                pageWorldBridge = resultWindowId == null,
                onPageStarted = { view ->
                    if (view === webView) {
                        pageReady = false
                        if (resultWindowId != null) pendingScripts.clear()
                        if (!gestureGate.onDocumentStarted() && untrustedView) { resultAssetPolicy?.audioPreview?.replace(null); resultCapabilityNotice.report(false) }
                    }
                },
            )
            if (resultWindowId != null) {
                if (untrustedView) {
                    val installed = installUntrustedResultGestureBridge(this, resultWindowId, resultIsDark) { message ->
                        if (untrustedResult && gestureGate.allows(targetView === webView, pageReady)) {
                            onMessage(message)
                        }
                    }
                    resultCapabilityNotice.report(installed)
                } else {
                    trustedResultBridgeAvailable = installTrustedResultBridge(
                        this, { it === webView && !untrustedResult }, ::dispatchBridgeMessage,
                    )
                    resultCapabilityNotice.report(trustedResultBridgeAvailable)
                }
            } else installPageWorldBridge(this, ::dispatchBridgeMessage)
        }
    }

    private fun dispatchBridgeMessage(message: String) {
        if (!handleOverlayHostMessage(appContext, message, logTag)) onMessage(message)
    }

    private fun replaceResultView(untrusted: Boolean, historyOffset: Int) {
        val old = webView
        pageReady = false
        pendingScripts.clear()
        runCatching { old.stopLoading() }
        rootView.removeView(old)
        untrustedResult = untrusted
        resultHistoryOffset = historyOffset
        managedLoadTracker = OverlayManagedLoadTracker()
        webView = createManagedWebView()
        inputFocusCoordinator = createInputFocusCoordinator()
        rootView.addView(webView, matchParentLayoutParams())
        runCatching { old.destroy() }
        if (attached && currentFocusable) inputFocusCoordinator.requestInitialFocus()
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
        if (deadView != null && deadView !== webView) {
            runCatching { deadView.destroy() }
            return true
        }
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
            managedLoadTracker = OverlayManagedLoadTracker()
            webView = createManagedWebView()
            inputFocusCoordinator = createInputFocusCoordinator()
            rootView.addView(webView, matchParentLayoutParams())
            if (currentFocusable && attached) {
                inputFocusCoordinator.requestInitialFocus()
            }
            when (source) {
                is OverlayRecoverySource.Asset -> loadAssetPage(source.assetPage)
                is OverlayRecoverySource.Html -> loadHtmlContent(source.htmlContent, source.baseUrl, untrustedContent = source.untrusted, audioPreview = source.audioPreview)
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

    private fun matchParentLayoutParams() = FrameLayout.LayoutParams(
        FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT,
    )

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
