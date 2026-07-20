package dev.screengoated.toolbox.mobile.phonecontrol.overlay

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Color
import android.webkit.JavascriptInterface
import android.webkit.RenderProcessGoneDetail
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import dev.screengoated.toolbox.mobile.phonecontrol.GeneratedPhoneControlContract
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog
import org.json.JSONObject

internal data class PhoneControlOrbPlacement(
    val centerXFraction: Float,
    val centerYFraction: Float,
    val magnification: Float,
)

@SuppressLint("SetJavaScriptEnabled")
internal class PhoneControlOrbView(
    context: Context,
    private val onRendererGone: (PhoneControlOrbView, Boolean) -> Unit,
) : WebView(context) {
    private var ready = false
    private var disposed = false
    private var visual: PhoneControlOverlayVisual? = null
    private var placement: PhoneControlOrbPlacement? = null
    private var appliedVisual: PhoneControlOverlayVisual? = null
    private var appliedPlacement: PhoneControlOrbPlacement? = null

    init {
        importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS
        isFocusable = false
        isFocusableInTouchMode = false
        setBackgroundColor(Color.TRANSPARENT)
        background = null
        isHorizontalScrollBarEnabled = false
        isVerticalScrollBarEnabled = false
        overScrollMode = OVER_SCROLL_NEVER
        setLayerType(LAYER_TYPE_HARDWARE, null)
        setRendererPriorityPolicy(RENDERER_PRIORITY_IMPORTANT, false)
        alpha = 0f
        settings.apply {
            javaScriptEnabled = true
            domStorageEnabled = false
            allowFileAccess = true
            allowContentAccess = false
            blockNetworkLoads = true
            cacheMode = android.webkit.WebSettings.LOAD_NO_CACHE
        }
        webViewClient = object : WebViewClient() {
            override fun shouldOverrideUrlLoading(
                view: WebView?,
                request: WebResourceRequest?,
            ): Boolean = true

            override fun onRenderProcessGone(
                view: WebView?,
                detail: RenderProcessGoneDetail?,
            ): Boolean {
                val crashed = detail?.didCrash() == true
                PhoneControlLog.e(TAG, "renderer_gone crashed=$crashed")
                post { if (!disposed) onRendererGone(this@PhoneControlOrbView, crashed) }
                return true
            }
        }
        addJavascriptInterface(OrbBridge(), IPC_BRIDGE)
        loadDataWithBaseURL(
            LOCAL_ORIGIN,
            canonicalRenderer(),
            "text/html",
            "utf-8",
            null,
        )
    }

    fun render(next: PhoneControlOverlayVisual, nextPlacement: PhoneControlOrbPlacement) {
        visual = next
        placement = nextPlacement
        if (ready) applyVisual(next, nextPlacement)
    }

    fun dispose() {
        if (disposed) return
        disposed = true
        ready = false
        removeJavascriptInterface(IPC_BRIDGE)
        stopLoading()
        destroy()
    }

    override fun onSizeChanged(width: Int, height: Int, oldWidth: Int, oldHeight: Int) {
        super.onSizeChanged(width, height, oldWidth, oldHeight)
        if (width == oldWidth && height == oldHeight) return
        appliedPlacement = null
        if (ready) {
            val nextVisual = visual ?: return
            val nextPlacement = placement ?: return
            applyVisual(nextVisual, nextPlacement)
        }
    }

    private fun canonicalRenderer(): String = context.assets
        .open(GeneratedPhoneControlContract.ORB_ASSET_PATH)
        .bufferedReader(Charsets.UTF_8)
        .use { it.readText() }
        .replace("/*FONT_CSS*/", ANDROID_RENDERER_CSS)
        .replace("/*CMD_PLACEHOLDER*/", "")

    private fun applyVisual(
        next: PhoneControlOverlayVisual,
        nextPlacement: PhoneControlOrbPlacement,
    ) {
        if (disposed) return
        val previous = appliedVisual
        val commands = mutableListOf<String>()
        if (appliedPlacement != nextPlacement) {
            commands += "window.cc.configurePlacement({" +
                "mag:${nextPlacement.magnification}," +
                "cxFrac:${nextPlacement.centerXFraction}," +
                "cyFrac:${nextPlacement.centerYFraction}})"
        }
        if (previous == null) commands += "window.cc.show()"
        if (previous?.stateLabel != next.stateLabel) {
            commands += "window.cc.setState(${JSONObject.quote(next.stateLabel)})"
        }
        if (previous?.iconOverride != next.iconOverride) {
            val icon = next.iconOverride?.let(JSONObject::quote) ?: "null"
            commands += "window.cc.setIcon($icon)"
        }
        if (previous?.caption != next.caption) {
            commands += "window.cc.setCaption(${JSONObject.quote(next.caption)})"
        }
        if (previous?.listeningLevel != next.listeningLevel) {
            commands += "window.cc.setAudio(${next.listeningLevel.coerceIn(0f, 1f)})"
        }
        if (commands.isNotEmpty()) {
            evaluateJavascript(commands.joinToString(separator = ";", postfix = ";"), null)
        }
        appliedVisual = next
        appliedPlacement = nextPlacement
    }

    private inner class OrbBridge {
        @JavascriptInterface
        fun postMessage(payload: String) {
            val type = runCatching { JSONObject(payload).optString("type") }.getOrNull()
            if (type != "orbReady") return
            post {
                if (disposed) return@post
                ready = true
                alpha = 1f
                val nextVisual = visual
                val nextPlacement = placement
                if (nextVisual != null && nextPlacement != null) {
                    applyVisual(nextVisual, nextPlacement)
                }
                PhoneControlLog.i(TAG, "renderer_ready source=canonical_windows surface=full_display")
            }
        }
    }

    private companion object {
        const val TAG = "SGTPhoneControlOverlay"
        const val IPC_BRIDGE = "ipc"
        const val LOCAL_ORIGIN = "file:///android_asset/phone_control/"
        const val ANDROID_RENDERER_CSS = """
            @font-face{font-family:'Google Sans Flex';src:url('../GoogleSansFlex.ttf') format('truetype');font-style:normal;font-weight:100 1000;}
            #c{pointer-events:none!important}
            #cmd{display:none!important}
        """
    }
}
