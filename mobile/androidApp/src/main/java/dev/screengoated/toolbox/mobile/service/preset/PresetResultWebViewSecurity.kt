package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import android.webkit.WebResourceResponse
import android.webkit.WebResourceRequest
import android.webkit.WebView
import androidx.webkit.WebViewAssetLoader
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import java.io.ByteArrayInputStream
import java.net.URI

internal const val RESULT_TRUSTED_ORIGIN = "https://sgt-result.invalid"
internal const val RESULT_TRUSTED_URL = "$RESULT_TRUSTED_ORIGIN/assets/preset_overlay/"

internal class TrustedResultScripts {
    private var documentToken = ""

    fun prepare(html: String): String {
        documentToken = java.util.UUID.randomUUID().toString()
        val guard = "<script>Object.defineProperty(window,'__SGT_RESULT_DOCUMENT__',{value:'$documentToken'});</script>"
        return html.replaceFirst("<head>", "<head>$guard")
    }

    fun wrap(script: String): String =
        "if(window.__SGT_RESULT_DOCUMENT__ === '$documentToken'){\n$script\n}"
}

internal class ResultCapabilityNotice(private val context: Context, private val message: String) {
    private var shown = false
    fun report(available: Boolean) {
        if (!available && !shown && message.isNotBlank()) {
            shown = true
            android.widget.Toast.makeText(context, message, android.widget.Toast.LENGTH_LONG).show()
        }
    }
}

internal class ResultDocumentGestureGate {
    private var started = false
    private var currentDocumentProven = true

    fun onDocumentStarted(): Boolean {
        if (started) currentDocumentProven = false
        started = true
        return currentDocumentProven
    }

    fun allows(activeView: Boolean, pageReady: Boolean): Boolean =
        activeView && pageReady && started && currentDocumentProven
}

// An untrusted WebView proves only its first document. A managed replacement
// must get a fresh view so old callbacks cannot be mistaken for its load.
internal fun needsFreshResultView(isResult: Boolean, wasUntrusted: Boolean, isUntrusted: Boolean): Boolean =
    isResult && (isUntrusted || wasUntrusted != isUntrusted)

internal fun revokePreviewOnNavigation(activeView: Boolean, preview: ResultAudioPreview?) {
    if (activeView) preview?.replace(null)
}

internal fun isTrustedResultUrl(url: String?): Boolean {
    val uri = runCatching { URI(url ?: return false) }.getOrNull() ?: return false
    return uri.scheme == "https" && uri.host == "sgt-result.invalid" &&
        uri.port == -1 && uri.userInfo == null && uri.path == "/assets/preset_overlay/"
}

internal fun isTrustedResultOrigin(origin: String?): Boolean {
    val uri = runCatching { URI(origin ?: return false) }.getOrNull() ?: return false
    return uri.scheme == "https" && uri.host == "sgt-result.invalid" &&
        uri.port in listOf(-1, 443) && uri.userInfo == null &&
        (uri.path.isNullOrEmpty() || uri.path == "/") && uri.query == null && uri.fragment == null
}

internal fun installTrustedResultBridge(
    view: WebView,
    isCurrentView: (WebView) -> Boolean,
    onMessage: (String) -> Unit,
): Boolean {
    if (!WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) return false
    return runCatching {
        WebViewCompat.addWebMessageListener(view, "sgtAndroid", setOf(RESULT_TRUSTED_ORIGIN)) {
            sender, message, sourceOrigin, isMainFrame, _ ->
            val value = message.data
            if (isMainFrame && value != null && isTrustedResultOrigin(sourceOrigin.toString()) &&
                isCurrentView(sender) && isTrustedResultUrl(sender.url)
            ) {
                onMessage(value)
            }
        }
        true
    }.getOrDefault(false)
}

internal class ResultAssetPolicy(context: Context) {
    val audioPreview = ResultAudioPreview()
    private val loader = WebViewAssetLoader.Builder()
        .setDomain("sgt-result.invalid")
        .addPathHandler("/assets/", WebViewAssetLoader.AssetsPathHandler(context))
        .build()

    fun intercept(request: WebResourceRequest, trusted: Boolean): WebResourceResponse? {
        val url = request.url
        if (!url.scheme.equals("https", ignoreCase = true) || !url.host.equals("sgt-result.invalid", ignoreCase = true)) return null
        audioPreview.intercept(request)?.let { return it }
        if (trusted) loader.shouldInterceptRequest(url)?.let { return it }
        return WebResourceResponse(
            "text/plain", "utf-8", 403, "Forbidden", emptyMap(), ByteArrayInputStream(ByteArray(0)),
        )
    }
}
