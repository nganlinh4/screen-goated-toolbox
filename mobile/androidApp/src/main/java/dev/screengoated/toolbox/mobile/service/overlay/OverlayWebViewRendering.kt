package dev.screengoated.toolbox.mobile.service.overlay

import android.view.View
import android.view.WindowManager
import android.webkit.WebView

internal fun overlayWebViewWindowFlags(baseFlags: Int): Int =
    baseFlags or WindowManager.LayoutParams.FLAG_HARDWARE_ACCELERATED

internal fun configureOverlayWebViewRendering(webView: WebView) {
    webView.setLayerType(View.LAYER_TYPE_NONE, null)
    webView.setRendererPriorityPolicy(WebView.RENDERER_PRIORITY_IMPORTANT, false)
}
