package dev.screengoated.toolbox.mobile.service.preset

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
import android.webkit.WebView
import android.webkit.WebViewClient
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
    @Suppress("DEPRECATION")
    webView.settings.allowFileAccessFromFileURLs = true
    webView.settings.mediaPlaybackRequiresUserGesture = false
    webView.settings.builtInZoomControls = false
    webView.settings.displayZoomControls = false
    webView.settings.setSupportZoom(false)
    webView.settings.useWideViewPort = false
    webView.settings.loadWithOverviewMode = false
    webView.settings.textZoom = 100
    webView.setLayerType(View.LAYER_TYPE_HARDWARE, null)
}

private fun isManagedWebViewUrl(url: String): Boolean {
    return url.startsWith("http://") ||
        url.startsWith("https://") ||
        url.startsWith("file:///android_asset/") ||
        url.startsWith("about:")
}

private fun saveMediaToDownloads(
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
        resolver.update(insertedUri, finalizeValues, null, null)
    } catch (error: Throwable) {
        insertedUri?.let { resolver.delete(it, null, null) }
        throw error
    }
}
