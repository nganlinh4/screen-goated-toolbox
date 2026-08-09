package dev.screengoated.toolbox.mobile.phonecontrol.overlay

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Color
import kotlin.math.ceil
import kotlin.math.floor
import android.webkit.JavascriptInterface
import android.webkit.RenderProcessGoneDetail
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import dev.screengoated.toolbox.mobile.phonecontrol.GeneratedPhoneControlContract
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog
import dev.screengoated.toolbox.mobile.service.overlay.configureOverlayWebViewRendering
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
    private val onVisibleRegionChanged: (PhoneControlOrbView, OverlayBounds?) -> Unit,
) : WebView(context) {
    private var ready = false
    private var disposed = false
    private var visual: PhoneControlOverlayVisual? = null
    private var placement: PhoneControlOrbPlacement? = null
    private var appliedVisual: PhoneControlOverlayVisual? = null
    private var appliedPlacement: PhoneControlOrbPlacement? = null
    private var renderScheduled = false
    private var dismissAnimating = false

    init {
        importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS
        isFocusable = false
        isFocusableInTouchMode = false
        setBackgroundColor(Color.TRANSPARENT)
        background = null
        isHorizontalScrollBarEnabled = false
        isVerticalScrollBarEnabled = false
        overScrollMode = OVER_SCROLL_NEVER
        configureOverlayWebViewRendering(this)
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
        scheduleVisualApply()
    }

    fun animateDismiss() {
        if (disposed) return
        dismissAnimating = true
        if (ready) {
            evaluateJavascript("window.cc.hide();", null)
        } else {
            alpha = 0f
        }
    }

    fun dispose() {
        if (disposed) return
        disposed = true
        ready = false
        renderScheduled = false
        onVisibleRegionChanged(this, null)
        removeJavascriptInterface(IPC_BRIDGE)
        stopLoading()
        destroy()
    }

    override fun onSizeChanged(width: Int, height: Int, oldWidth: Int, oldHeight: Int) {
        super.onSizeChanged(width, height, oldWidth, oldHeight)
        if (width == oldWidth && height == oldHeight) return
        appliedPlacement = null
        scheduleVisualApply()
    }

    private fun scheduleVisualApply() {
        if (disposed || dismissAnimating || !ready || renderScheduled) return
        renderScheduled = true
        postOnAnimation {
            renderScheduled = false
            if (disposed || dismissAnimating || !ready) return@postOnAnimation
            applyVisual(visual ?: return@postOnAnimation, placement ?: return@postOnAnimation)
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
            val message = runCatching { JSONObject(payload) }.getOrNull() ?: return
            when (message.optString("type")) {
                "orbReady" -> post {
                    if (disposed) return@post
                    ready = true
                    alpha = 1f
                    scheduleVisualApply()
                    PhoneControlLog.i(
                        TAG,
                        "renderer_ready source=canonical_windows surface=full_display",
                    )
                }
                "orbRegion" -> publishVisibleRegion(message)
            }
        }

        private fun publishVisibleRegion(message: JSONObject) {
            val region = rendererRegionInView(message) ?: return
            post {
                if (!disposed) onVisibleRegionChanged(this@PhoneControlOrbView, region)
            }
        }
    }

    private fun rendererRegionInView(message: JSONObject): OverlayBounds? {
        val viewportWidth = message.optDouble("viewportW")
        val viewportHeight = message.optDouble("viewportH")
        val x = message.optDouble("x")
        val y = message.optDouble("y")
        val regionWidth = message.optDouble("w")
        val regionHeight = message.optDouble("h")
        if (!listOf(
                viewportWidth,
                viewportHeight,
                x,
                y,
                regionWidth,
                regionHeight,
            ).all(Double::isFinite) ||
            viewportWidth <= 0.0 ||
            viewportHeight <= 0.0 ||
            regionWidth <= 0.0 ||
            regionHeight <= 0.0 ||
            width <= 0 ||
            height <= 0
        ) {
            return null
        }
        return scaleRendererRegion(
            x = x,
            y = y,
            regionWidth = regionWidth,
            regionHeight = regionHeight,
            viewportWidth = viewportWidth,
            viewportHeight = viewportHeight,
            viewWidth = width,
            viewHeight = height,
        )
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

internal fun scaleRendererRegion(
    x: Double,
    y: Double,
    regionWidth: Double,
    regionHeight: Double,
    viewportWidth: Double,
    viewportHeight: Double,
    viewWidth: Int,
    viewHeight: Int,
): OverlayBounds? {
    if (viewportWidth <= 0.0 ||
        viewportHeight <= 0.0 ||
        regionWidth <= 0.0 ||
        regionHeight <= 0.0 ||
        viewWidth <= 0 ||
        viewHeight <= 0
    ) {
        return null
    }
    val left = floor(x * viewWidth / viewportWidth).toInt().coerceIn(0, viewWidth)
    val top = floor(y * viewHeight / viewportHeight).toInt().coerceIn(0, viewHeight)
    val right = ceil((x + regionWidth) * viewWidth / viewportWidth)
        .toInt()
        .coerceIn(0, viewWidth)
    val bottom = ceil((y + regionHeight) * viewHeight / viewportHeight)
        .toInt()
        .coerceIn(0, viewHeight)
    return if (right > left && bottom > top) {
        OverlayBounds(left, top, right, bottom)
    } else {
        null
    }
}
