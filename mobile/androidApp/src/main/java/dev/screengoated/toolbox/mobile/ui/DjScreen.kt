package dev.screengoated.toolbox.mobile.ui

import android.annotation.SuppressLint
import android.os.Handler
import android.os.Looper
import android.view.View
import android.view.ViewGroup
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.viewinterop.AndroidView

@SuppressLint("SetJavaScriptEnabled")
@Composable
internal fun DjScreen(
    apiKey: String,
    isDark: Boolean,
    lang: String,
    onBack: () -> Unit,
) {
    val context = LocalContext.current
    val handler = remember { Handler(Looper.getMainLooper()) }
    val webView = remember {
        WebView(context).apply {
            layoutParams = ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT,
            )
            isVerticalScrollBarEnabled = true
            isHorizontalScrollBarEnabled = false
            overScrollMode = WebView.OVER_SCROLL_IF_CONTENT_SCROLLS
            setLayerType(View.LAYER_TYPE_HARDWARE, null)
            setBackgroundColor(android.graphics.Color.TRANSPARENT)

            settings.javaScriptEnabled = true
            settings.domStorageEnabled = true
            settings.mediaPlaybackRequiresUserGesture = false
            settings.allowFileAccess = true
            @Suppress("DEPRECATION")
            settings.allowUniversalAccessFromFileURLs = true
            settings.builtInZoomControls = false
            settings.displayZoomControls = false
            settings.setSupportZoom(false)

            webChromeClient = object : WebChromeClient() {
                override fun onJsAlert(
                    view: WebView?,
                    url: String?,
                    message: String?,
                    result: android.webkit.JsResult?,
                ): Boolean {
                    result?.cancel()
                    return true
                }
            }
        }
    }

    // Inject API key, theme, and language after page finishes loading
    val escapedKey = remember(apiKey) { escapeJs(apiKey) }
    val theme = if (isDark) "dark" else "light"

    DisposableEffect(webView) {
        webView.webViewClient = object : WebViewClient() {
            override fun onPageFinished(view: WebView?, url: String?) {
                super.onPageFinished(view, url)
                handler.postDelayed({
                    val script = """
                        window.postMessage({ type: 'pm-dj-set-api-key', apiKey: '$escapedKey', lang: '$lang' }, '*');
                        window.postMessage({ type: 'pm-dj-set-theme', theme: '$theme' }, '*');
                        window.postMessage({ type: 'pm-dj-set-font', font: 'google-sans-flex' }, '*');
                    """.trimIndent()
                    webView.evaluateJavascript(script, null)
                }, 300)
            }

            override fun shouldOverrideUrlLoading(view: WebView?, request: WebResourceRequest?): Boolean {
                // Allow all loads within the WebView (CDN imports, etc.)
                return false
            }
        }
        webView.loadUrl("file:///android_asset/promptdj/index.html")

        onDispose {
            handler.removeCallbacksAndMessages(null)
            webView.evaluateJavascript(
                "window.postMessage({ type: 'pm-dj-stop-audio' }, '*')",
                null,
            )
            webView.stopLoading()
            webView.destroy()
        }
    }

    BackHandler {
        webView.evaluateJavascript(
            "window.postMessage({ type: 'pm-dj-stop-audio' }, '*')",
            null,
        )
        onBack()
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.surface),
    ) {
        AndroidView(
            factory = { webView },
            modifier = Modifier.fillMaxSize(),
        )
    }
}

private fun escapeJs(value: String): String =
    value.replace("\\", "\\\\")
        .replace("'", "\\'")
        .replace("\n", "\\n")
        .replace("\r", "\\r")
