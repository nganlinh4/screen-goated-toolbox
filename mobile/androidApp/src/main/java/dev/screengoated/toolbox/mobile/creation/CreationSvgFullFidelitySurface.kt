package dev.screengoated.toolbox.mobile.creation

import android.graphics.Paint
import android.net.Uri
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.viewinterop.AndroidView
import java.io.ByteArrayInputStream
import java.util.UUID
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

@Composable
internal fun CreationSvgFullFidelitySurface(
    svg: String,
    document: NativeSvgDocument?,
    controller: CreationSvgDocumentController,
    revision: Int,
    modifier: Modifier = Modifier,
) {
    val snapshot = remember(svg, document, revision) {
        document?.serializationSnapshot()?.serialize() ?: svg
    }
    val payload by produceState<SvgPreviewPayload?>(null, snapshot) {
        value = withContext(Dispatchers.Default) {
            createSvgPreviewPayload(snapshot)
        }
    }
    val context = LocalContext.current
    val accent = androidx.compose.material3.MaterialTheme.colorScheme.primary
    val resourceClient = remember { SvgPreviewResourceClient() }
    val webView = remember {
        WebView(context).apply {
            setBackgroundColor(android.graphics.Color.TRANSPARENT)
            settings.apply {
                javaScriptEnabled = false
                allowFileAccess = false
                allowContentAccess = false
                blockNetworkLoads = true
                builtInZoomControls = false
                displayZoomControls = false
                setSupportMultipleWindows(false)
            }
            removeJavascriptInterface("searchBoxJavaBridge_")
            removeJavascriptInterface("accessibility")
            removeJavascriptInterface("accessibilityTraversal")
            webViewClient = resourceClient
        }
    }
    DisposableEffect(webView) {
        onDispose {
            webView.stopLoading()
            webView.loadUrl("about:blank")
            webView.destroy()
        }
    }

    Box(modifier) {
        AndroidView(
            factory = { webView },
            modifier = Modifier.fillMaxSize(),
            update = { view ->
                val currentPayload = payload ?: return@AndroidView
                view.scaleX = controller.zoom
                view.scaleY = controller.zoom
                view.translationX = controller.pan.x
                view.translationY = controller.pan.y
                resourceClient.payload = currentPayload
                if (view.tag != currentPayload.revisionId) {
                    view.tag = currentPayload.revisionId
                    view.loadDataWithBaseURL(
                        null,
                        svgPreviewHtml(currentPayload.resourceUrl),
                        "text/html",
                        "UTF-8",
                        null,
                    )
                }
            },
        )
        val transformGestures = Modifier
            .fillMaxSize()
            .pointerInput(svg, revision) {
                detectTransformGestures { _, pan, zoom, _ ->
                    controller.transform(pan, zoom)
                }
            }
        Canvas(
            modifier = if (document == null) {
                transformGestures
            } else {
                transformGestures
                    .pointerInput(document, revision) {
                        detectTapGestures { point ->
                            val transform = document.viewportTransform(
                                size.width.toFloat(),
                                size.height.toFloat(),
                                controller.zoom,
                                controller.pan,
                            )
                            controller.select(
                                document.hitTest(
                                    transform.toDocument(point),
                                    transform.documentTolerance(8f),
                                ),
                            )
                        }
                    }
            },
        ) {
            val editableDocument = document ?: return@Canvas
            val selected = controller.selectedIndex?.let(editableDocument.shapes::getOrNull)
                ?.takeUnless { it.deleted }
                ?: return@Canvas
            val transform = editableDocument.viewportTransform(
                size.width,
                size.height,
                controller.zoom,
                controller.pan,
            )
            drawIntoCanvas { composeCanvas ->
                val canvas = composeCanvas.nativeCanvas
                canvas.save()
                canvas.translate(
                    transform.origin.x + transform.pan.x,
                    transform.origin.y + transform.pan.y,
                )
                canvas.scale(transform.scale, transform.scale)
                canvas.translate(-editableDocument.viewBox.left, -editableDocument.viewBox.top)
                canvas.drawPath(
                    selected.path,
                    Paint(Paint.ANTI_ALIAS_FLAG).apply {
                        style = Paint.Style.STROKE
                        color = accent.toArgb()
                        strokeWidth = 2.2f / transform.scale
                    },
                )
                canvas.restore()
            }
        }
    }
}

internal data class SvgPreviewPayload(
    val revisionId: String,
    val resourceUrl: String,
    val bytes: ByteArray,
)

internal fun createSvgPreviewPayload(svg: String): SvgPreviewPayload {
    val bytes = svg.encodeToByteArray()
    require(bytes.size.toLong() <= CreationContract.MAXIMUM_SVG_ARTIFACT_BYTES)
    val revisionId = UUID.randomUUID().toString()
    return SvgPreviewPayload(
        revisionId,
        "$SVG_PREVIEW_ORIGIN/$revisionId.svg",
        bytes,
    )
}

internal fun svgPreviewHtml(resourceUrl: String): String {
    require(resourceUrl.startsWith("$SVG_PREVIEW_ORIGIN/"))
    return """
    <!doctype html>
    <html>
      <head>
        <meta name="viewport" content="width=device-width,initial-scale=1">
        <meta http-equiv="Content-Security-Policy"
              content="default-src 'none'; style-src 'unsafe-inline'; img-src $SVG_PREVIEW_ORIGIN">
        <style>
          html,body { width:100%; height:100%; margin:0; overflow:hidden; background:transparent; }
          body { display:flex; align-items:center; justify-content:center; }
          img {
            width:94vw; height:94vh; object-fit:contain;
            animation:sgt-reveal 180ms ease-out both;
          }
          @keyframes sgt-reveal { from { opacity:0 } to { opacity:1 } }
          @media (prefers-reduced-motion:reduce) { img { animation:none } }
        </style>
      </head>
      <body><img alt="" src="$resourceUrl"></body>
    </html>
    """.trimIndent()
}

private class SvgPreviewResourceClient : WebViewClient() {
    @Volatile var payload: SvgPreviewPayload? = null

    override fun shouldOverrideUrlLoading(
        view: WebView?,
        request: WebResourceRequest?,
    ): Boolean = true

    override fun shouldInterceptRequest(
        view: WebView?,
        request: WebResourceRequest?,
    ): WebResourceResponse {
        val current = payload
        return if (current != null && request?.url == Uri.parse(current.resourceUrl)) {
            WebResourceResponse(
                "image/svg+xml",
                "UTF-8",
                ByteArrayInputStream(current.bytes),
            )
        } else {
            WebResourceResponse("text/plain", "UTF-8", ByteArrayInputStream(ByteArray(0)))
        }
    }
}

private const val SVG_PREVIEW_ORIGIN = "https://appassets.androidplatform.net/creation"
