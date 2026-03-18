package dev.screengoated.toolbox.mobile.service.preset

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.util.Log
import android.view.View
import android.view.WindowManager
import android.view.inputmethod.InputMethodManager
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient

internal class OverlayManagedLoadTracker {
    private var clearHistoryOnNextPageFinished = false
    private var managedLoadInFlight = false
    private var managedLoadPrefix: String? = null

    fun begin(prefix: String, clearHistoryAfterLoad: Boolean = false) {
        clearHistoryOnNextPageFinished = clearHistoryAfterLoad
        managedLoadInFlight = true
        managedLoadPrefix = prefix
    }

    fun shouldIgnorePageFinished(url: String?): Boolean {
        return managedLoadInFlight && !matches(url)
    }

    fun finish(view: WebView?) {
        managedLoadInFlight = false
        managedLoadPrefix = null
        if (clearHistoryOnNextPageFinished) {
            clearHistoryOnNextPageFinished = false
            runCatching { view?.clearHistory() }
        }
    }

    private fun matches(url: String?): Boolean {
        val prefix = managedLoadPrefix ?: return true
        val currentUrl = url ?: return false
        return currentUrl.startsWith(prefix)
    }
}

internal class OverlayInputFocusCoordinator(
    context: Context,
    private val webView: WebView,
    private val focusable: Boolean,
    private val showImeOnFocus: Boolean,
) {
    private val appContext = context.applicationContext

    fun requestInitialFocus() {
        focusWebView(showKeyboard = showImeOnFocus)
        if (showImeOnFocus) {
            webView.postDelayed({ focusWebView(showKeyboard = true) }, 120L)
            webView.postDelayed({ focusWebView(showKeyboard = true) }, 260L)
        }
    }

    fun refocus(refocusIme: Boolean) {
        if (!focusable) return
        if (refocusIme) {
            requestInitialFocus()
        } else {
            focusWebView(showKeyboard = false)
        }
    }

    fun hideKeyboard() {
        if (!focusable) return
        val inputMethodManager = appContext.getSystemService(InputMethodManager::class.java)
        inputMethodManager?.hideSoftInputFromWindow(webView.windowToken, 0)
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
}

internal fun createOverlayWebViewClient(
    appContext: Context,
    logTag: String,
    tracker: OverlayManagedLoadTracker,
    onMessageLog: (String) -> Unit = {},
    onMainFrameNavigationFailure: (OverlayNavigationFailure) -> Unit,
    onPageFinished: (String?) -> Unit,
): WebViewClient {
    return object : WebViewClient() {
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

        override fun onPageStarted(
            view: WebView?,
            url: String?,
            favicon: android.graphics.Bitmap?,
        ) {
            onMessageLog("pageStarted url=${url ?: "null"}")
        }

        override fun onPageFinished(
            view: WebView?,
            url: String?,
        ) {
            onMessageLog("pageFinished url=${url ?: "null"}")
            if (tracker.shouldIgnorePageFinished(url)) {
                onMessageLog("pageFinished ignored stale url=${url ?: "null"}")
                return
            }
            tracker.finish(view)
            onPageFinished(url)
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
                onMainFrameNavigationFailure(
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
                onMainFrameNavigationFailure(
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
}

internal fun buildOverlayWindowFlags(focusable: Boolean): Int {
    var flags = WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
        WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL
    if (!focusable) {
        flags = flags or WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE
    }
    return flags
}

internal fun configureOverlayWebView(
    webView: WebView,
    focusable: Boolean,
) {
    webView.overScrollMode = View.OVER_SCROLL_NEVER
    webView.setBackgroundColor(android.graphics.Color.TRANSPARENT)
    webView.isVerticalScrollBarEnabled = false
    webView.isHorizontalScrollBarEnabled = false
    webView.isFocusable = focusable
    webView.isFocusableInTouchMode = focusable
    webView.settings.javaScriptEnabled = true
    webView.settings.domStorageEnabled = true
    webView.settings.allowFileAccess = true
    webView.settings.mediaPlaybackRequiresUserGesture = false
    webView.settings.builtInZoomControls = false
    webView.settings.displayZoomControls = false
    webView.settings.setSupportZoom(false)
    webView.setLayerType(View.LAYER_TYPE_HARDWARE, null)
}

private fun isManagedWebViewUrl(url: String): Boolean {
    return url.startsWith("http://") ||
        url.startsWith("https://") ||
        url.startsWith("file:///android_asset/") ||
        url.startsWith("about:")
}
