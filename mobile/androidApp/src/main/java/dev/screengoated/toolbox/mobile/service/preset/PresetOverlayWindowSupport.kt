package dev.screengoated.toolbox.mobile.service.preset

import android.annotation.SuppressLint
import android.content.ActivityNotFoundException
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Environment
import android.provider.MediaStore
import android.util.Log
import android.widget.Toast
import android.view.View
import android.view.WindowManager
import android.view.inputmethod.InputMethodManager
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.core.net.toUri
import dev.screengoated.toolbox.mobile.service.overlay.configureOverlayWebViewRendering
import org.json.JSONObject
import java.util.Base64

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

/**
 * Page-world `sgtAndroid` bridge for non-result overlay windows. It is visible to every frame
 * and origin, so those windows pass pageWorldBridge = true and never keep a web page loaded.
 */
internal fun installPageWorldBridge(view: WebView, onMessage: (String) -> Unit) {
    view.addJavascriptInterface(
        object {
            @android.webkit.JavascriptInterface
            fun postMessage(message: String) {
                view.post { onMessage(message) }
            }
        },
        "sgtAndroid",
    )
}

internal fun createOverlayWebViewClient(
    appContext: Context,
    logTag: String,
    tracker: OverlayManagedLoadTracker,
    onMessageLog: (String) -> Unit = {},
    onRenderProcessGone: (WebView?, android.webkit.RenderProcessGoneDetail?) -> Boolean = { _, _ -> false },
    onMainFrameNavigationFailure: (OverlayNavigationFailure) -> Unit,
    onPageFinished: (String?) -> Unit,
    onMainFrameNavigation: (WebView?, WebResourceRequest) -> Boolean = { _, _ -> false },
    interceptRequest: (WebResourceRequest) -> WebResourceResponse? = { null },
    onPageStarted: (WebView?) -> Unit = {},
    pageWorldBridge: Boolean = false,
): WebViewClient {
    return object : WebViewClient() {
        override fun shouldOverrideUrlLoading(
            view: WebView?,
            request: WebResourceRequest?,
        ): Boolean {
            val url = request?.url?.toString().orEmpty()
            if (request != null && request.isForMainFrame && onMainFrameNavigation(view, request)) {
                return true
            }
            when (overlayNavigationPolicy(url, request?.hasGesture() == true, pageWorldBridge)) {
                OverlayNavigationPolicy.MANAGED -> return false
                OverlayNavigationPolicy.BLOCK -> {
                    Log.w(logTag, "Blocked external navigation (local resource scheme or no user gesture)")
                    return true
                }
                OverlayNavigationPolicy.EXTERNAL_INTENT -> Unit
            }
            val intent = Intent(Intent.ACTION_VIEW, url.toUri()).apply {
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            return try {
                appContext.startActivity(intent)
                true
            } catch (_: ActivityNotFoundException) {
                Log.w(logTag, "No activity found for external url=$url")
                true
            } catch (error: RuntimeException) {
                Log.w(logTag, "External navigation refused url=$url", error)
                true
            }
        }

        override fun shouldInterceptRequest(view: WebView?, request: WebResourceRequest?): WebResourceResponse? {
            return request?.let(interceptRequest) ?: super.shouldInterceptRequest(view, request)
        }

        override fun onPageStarted(
            view: WebView?,
            url: String?,
            favicon: android.graphics.Bitmap?,
        ) {
            onPageStarted(view)
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
            return onRenderProcessGone(view, detail)
        }
    }
}

internal enum class OverlayNavigationPolicy { MANAGED, EXTERNAL_INTENT, BLOCK }

/**
 * [pageWorldBridge]: the view exposes `sgtAndroid` through addJavascriptInterface (every
 * frame, every origin), so it may only ever show bundled pages; web links go to the browser.
 */
internal fun overlayNavigationPolicy(
    url: String,
    hasUserGesture: Boolean,
    pageWorldBridge: Boolean = false,
): OverlayNavigationPolicy = when {
    url.isBlank() -> OverlayNavigationPolicy.MANAGED
    pageWorldBridge && isWebUrl(url) ->
        if (hasUserGesture) OverlayNavigationPolicy.EXTERNAL_INTENT else OverlayNavigationPolicy.BLOCK
    isManagedWebViewUrl(url) -> OverlayNavigationPolicy.MANAGED
    // Local file/content URIs cannot be handed to another app (FileUriExposedException,
    // or an unintended grant of app-private content), even with a gesture.
    isLocalResourceUrl(url) -> OverlayNavigationPolicy.BLOCK
    hasUserGesture -> OverlayNavigationPolicy.EXTERNAL_INTENT
    else -> OverlayNavigationPolicy.BLOCK
}

internal fun handleOverlayHostMessage(
    appContext: Context,
    message: String,
    logTag: String,
): Boolean {
    val payload = runCatching { JSONObject(message) }.getOrNull() ?: return false
    if (payload.optString("type") != "saveMediaToDownloads") {
        return false
    }

    val filename = payload.optString("filename").ifBlank { "download.bin" }
    val mimeType = payload.optString("mimeType").ifBlank { "application/octet-stream" }
    val base64 = payload.optString("base64")
    val successMessage = payload.optString("successMessage").ifBlank { "Downloaded" }
    val failureMessage = payload.optString("failureMessage").ifBlank { "Could not download" }
    if (base64.isBlank()) {
        Toast.makeText(appContext, failureMessage, Toast.LENGTH_SHORT).show()
        return true
    }

    val bytes = runCatching { Base64.getDecoder().decode(base64) }
        .onFailure { error -> Log.e(logTag, "Failed to decode media download payload", error) }
        .getOrNull()
    if (bytes == null) {
        Toast.makeText(appContext, failureMessage, Toast.LENGTH_SHORT).show()
        return true
    }

    val saved = runCatching {
        saveMediaToDownloads(
            appContext = appContext,
            filename = filename,
            mimeType = mimeType,
            bytes = bytes,
        )
    }.onFailure { error ->
        Log.e(logTag, "Failed to save media download", error)
    }.isSuccess

    Toast.makeText(
        appContext,
        if (saved) successMessage else failureMessage,
        Toast.LENGTH_SHORT,
    ).show()
    return true
}

internal fun buildOverlayWindowFlags(
    focusable: Boolean,
    watchOutsideTouch: Boolean = false,
): Int {
    var flags = WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
        WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL
    if (watchOutsideTouch) {
        flags = flags or WindowManager.LayoutParams.FLAG_WATCH_OUTSIDE_TOUCH
    }
    if (!focusable) {
        flags = flags or WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE
    }
    return flags
}

@SuppressLint("SetJavaScriptEnabled")
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
    webView.settings.mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
    webView.settings.javaScriptEnabled = true
    webView.settings.domStorageEnabled = true
    webView.settings.allowFileAccess = true
    webView.settings.allowContentAccess = false
    @Suppress("DEPRECATION")
    webView.settings.allowFileAccessFromFileURLs = false
    @Suppress("DEPRECATION")
    webView.settings.allowUniversalAccessFromFileURLs = false
    webView.settings.mediaPlaybackRequiresUserGesture = false
    webView.settings.builtInZoomControls = false
    webView.settings.displayZoomControls = false
    webView.settings.setSupportZoom(false)
    webView.settings.useWideViewPort = false
    webView.settings.loadWithOverviewMode = false
    webView.settings.textZoom = 100
    configureOverlayWebViewRendering(webView)
}

private fun isLocalResourceUrl(url: String): Boolean {
    val scheme = url.substringBefore(':', missingDelimiterValue = "").trim().lowercase()
    return scheme == "file" || scheme == "content"
}

private fun isWebUrl(url: String): Boolean =
    url.startsWith("http://", ignoreCase = true) || url.startsWith("https://", ignoreCase = true)

private fun isManagedWebViewUrl(url: String): Boolean {
    return isWebUrl(url) ||
        url.startsWith("file:///android_asset/") ||
        url.startsWith("about:")
}

internal fun saveMediaToDownloads(
    appContext: Context,
    filename: String,
    mimeType: String,
    bytes: ByteArray,
) {
    val resolver = appContext.contentResolver
    val values = ContentValues().apply {
        put(MediaStore.MediaColumns.DISPLAY_NAME, filename)
        put(MediaStore.MediaColumns.MIME_TYPE, mimeType)
        put(MediaStore.MediaColumns.RELATIVE_PATH, "${Environment.DIRECTORY_DOWNLOADS}/Screen Goated Toolbox")
        put(MediaStore.MediaColumns.IS_PENDING, 1)
    }
    var insertedUri: Uri? = null
    try {
        insertedUri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
            ?: error("Could not create download row.")
        resolver.openOutputStream(insertedUri)?.use { output ->
            output.write(bytes)
        } ?: error("Could not open download output stream.")
        val finalizeValues = ContentValues().apply {
            put(MediaStore.MediaColumns.IS_PENDING, 0)
        }
        check(resolver.update(insertedUri, finalizeValues, null, null) > 0) {
            "Could not finalize download."
        }
    } catch (error: Throwable) {
        insertedUri?.let { resolver.delete(it, null, null) }
        throw error
    }
}
