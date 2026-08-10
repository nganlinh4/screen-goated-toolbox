package dev.screengoated.toolbox.mobile.creation

import android.content.res.AssetManager
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import dev.screengoated.toolbox.mobile.ui.i18n.Creation3dLocale
import java.io.ByteArrayInputStream
import java.io.File
import java.io.FileInputStream
import java.net.URI
import java.security.SecureRandom
import org.json.JSONObject

internal const val CREATION_MODEL_VIEWER_DOCUMENT_VERSION = 1
internal const val CREATION_MODEL_VIEWER_ORIGIN =
    "https://appassets.androidplatform.net/creation-model-viewer"
private const val CREATION_MODEL_VIEWER_ASSET_ROOT = "creation_model_viewer"
private const val CREATION_MODEL_VIEWER_CSP =
    "default-src 'none'; script-src 'self'; style-src 'self'; font-src 'self'; connect-src 'self'; " +
        "img-src 'self' blob: data:; object-src 'none'; base-uri 'none'; " +
        "form-action 'none'; frame-src 'none'; worker-src 'none'"
private val CREATION_MODEL_VIEWER_TOKEN = Regex("[0-9a-f]{32}")

internal data class CreationModelViewerSession(
    val token: String,
    val modelFile: File,
    val segmented: Boolean,
    val darkTheme: Boolean,
    val labels: CreationModelViewerLabels,
) {
    val documentUrl: String =
        "$CREATION_MODEL_VIEWER_ORIGIN/v$CREATION_MODEL_VIEWER_DOCUMENT_VERSION/$token/index.html"
    val modelUrl: String =
        "$CREATION_MODEL_VIEWER_ORIGIN/v$CREATION_MODEL_VIEWER_DOCUMENT_VERSION/$token/model.glb"

    companion object {
        fun create(
            modelFile: File,
            segmented: Boolean,
            darkTheme: Boolean,
            strings: Creation3dLocale,
        ) = CreationModelViewerSession(
            token = ByteArray(16).also(SecureRandom()::nextBytes)
                .joinToString("") { byte -> "%02x".format(byte) },
            modelFile = modelFile,
            segmented = segmented,
            darkTheme = darkTheme,
            labels = CreationModelViewerLabels.from(strings),
        )
    }
}

internal data class CreationModelViewerLabels(
    val originalMaterials: String,
    val toonOutline: String,
    val partColors: String,
    val toggleOutline: String,
    val toggleRotation: String,
    val toggleGrid: String,
    val toggleWireframe: String,
    val resetView: String,
    val preview: String,
    val previewUnavailable: String,
) {
    fun json() = JSONObject()
        .put("originalMaterials", originalMaterials)
        .put("toonOutline", toonOutline)
        .put("partColors", partColors)
        .put("toggleOutline", toggleOutline)
        .put("toggleRotation", toggleRotation)
        .put("toggleGrid", toggleGrid)
        .put("toggleWireframe", toggleWireframe)
        .put("resetView", resetView)
        .put("preview", preview)
        .put("previewUnavailable", previewUnavailable)

    companion object {
        fun from(strings: Creation3dLocale) = CreationModelViewerLabels(
            originalMaterials = strings.originalMaterials,
            toonOutline = strings.toonOutline,
            partColors = strings.partColors,
            toggleOutline = strings.outline,
            toggleRotation = strings.autoRotate,
            toggleGrid = strings.grid,
            toggleWireframe = strings.wireframe,
            resetView = strings.fit,
            preview = strings.modelReady,
            previewUnavailable = strings.previewUnavailable,
        )
    }
}

internal enum class CreationModelViewerResource(val assetPath: String?) {
    DOCUMENT("$CREATION_MODEL_VIEWER_ASSET_ROOT/index.html"),
    SCRIPT("$CREATION_MODEL_VIEWER_ASSET_ROOT/assets/viewer.js"),
    STYLE("$CREATION_MODEL_VIEWER_ASSET_ROOT/assets/viewer.css"),
    FONT("GoogleSansFlex.woff"),
    MODEL(null),
}

internal fun routeCreationModelViewerRequest(
    url: String,
    method: String,
    token: String,
): CreationModelViewerResource? {
    if (method != "GET" || !CREATION_MODEL_VIEWER_TOKEN.matches(token)) return null
    val uri = runCatching { URI(url) }.getOrNull() ?: return null
    if (
        uri.scheme != "https" ||
        uri.rawAuthority != "appassets.androidplatform.net" ||
        uri.rawQuery != null ||
        uri.rawFragment != null
    ) return null
    val root = "/creation-model-viewer/v$CREATION_MODEL_VIEWER_DOCUMENT_VERSION/$token"
    return when (uri.rawPath) {
        "/creation-model-viewer/GoogleSansFlex.woff" -> CreationModelViewerResource.FONT
        "$root/index.html" -> CreationModelViewerResource.DOCUMENT
        "$root/assets/viewer.js" -> CreationModelViewerResource.SCRIPT
        "$root/assets/viewer.css" -> CreationModelViewerResource.STYLE
        "$root/model.glb" -> CreationModelViewerResource.MODEL
        else -> null
    }
}

internal fun creationModelViewerStartScript(session: CreationModelViewerSession): String {
    val options = JSONObject()
        .put("modelUrl", session.modelUrl)
        .put("segmented", session.segmented)
        .put("theme", if (session.darkTheme) "dark" else "light")
        .put("labels", session.labels.json())
    return "window.sgtModelViewer?.start($options)"
}

internal class CreationModelViewerClient(
    private val assets: AssetManager,
    private val session: CreationModelViewerSession,
) : WebViewClient() {
    override fun shouldOverrideUrlLoading(
        view: WebView?,
        request: WebResourceRequest?,
    ): Boolean = request?.url?.toString() != session.documentUrl

    override fun onPageFinished(view: WebView, url: String) {
        if (url == session.documentUrl) {
            view.evaluateJavascript(creationModelViewerStartScript(session), null)
        }
    }

    override fun shouldInterceptRequest(
        view: WebView?,
        request: WebResourceRequest?,
    ): WebResourceResponse {
        val resource = request?.let {
            routeCreationModelViewerRequest(it.url.toString(), it.method, session.token)
        } ?: return blockedResponse()
        return runCatching {
            when (resource) {
                CreationModelViewerResource.DOCUMENT -> assetResponse(resource, "text/html")
                CreationModelViewerResource.SCRIPT -> assetResponse(resource, "text/javascript")
                CreationModelViewerResource.STYLE -> assetResponse(resource, "text/css")
                CreationModelViewerResource.FONT -> assetResponse(resource, "font/woff", null)
                CreationModelViewerResource.MODEL -> modelResponse()
            }
        }.getOrElse { blockedResponse() }
    }

    private fun assetResponse(
        resource: CreationModelViewerResource,
        mimeType: String,
        encoding: String? = "UTF-8",
    ) = successfulResponse(
        mimeType = mimeType,
        encoding = encoding,
        stream = assets.open(requireNotNull(resource.assetPath), AssetManager.ACCESS_STREAMING),
    )

    private fun modelResponse(): WebResourceResponse {
        CreationArtifactValidator.validateGlb(session.modelFile)
        val stream = FileInputStream(session.modelFile)
        return try {
            val length = stream.channel.size()
            require(length in 20..CreationContract.MAXIMUM_GLB_ARTIFACT_BYTES)
            successfulResponse(
                mimeType = "model/gltf-binary",
                encoding = null,
                stream = stream,
                extraHeaders = mapOf("Content-Length" to length.toString()),
            )
        } catch (error: Throwable) {
            stream.close()
            throw error
        }
    }
}

private fun successfulResponse(
    mimeType: String,
    encoding: String?,
    stream: java.io.InputStream,
    extraHeaders: Map<String, String> = emptyMap(),
) = WebResourceResponse(
    mimeType,
    encoding,
    200,
    "OK",
    mapOf(
        "Cache-Control" to "no-store",
        "Content-Security-Policy" to CREATION_MODEL_VIEWER_CSP,
        "Cross-Origin-Resource-Policy" to "same-origin",
        "X-Content-Type-Options" to "nosniff",
    ) + extraHeaders,
    stream,
)

private fun blockedResponse() = WebResourceResponse(
    "text/plain",
    "UTF-8",
    403,
    "Blocked",
    mapOf("Cache-Control" to "no-store"),
    ByteArrayInputStream(ByteArray(0)),
)
