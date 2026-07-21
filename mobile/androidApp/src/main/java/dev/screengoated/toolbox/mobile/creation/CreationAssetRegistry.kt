package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import android.net.Uri
import android.webkit.WebResourceResponse
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap

internal class CreationAssetRegistry(
    context: Context,
    private val files: CreationFileStore,
) {
    private val assetManager = context.applicationContext.assets
    private val assets = ConcurrentHashMap<String, Asset>()

    fun register(path: String): String {
        val token = UUID.randomUUID().toString()
        assets[token] = Asset(path, mimeType(path))
        return "$ORIGIN/$token"
    }

    fun intercept(uri: Uri): WebResourceResponse? {
        if (uri.scheme != "https" || uri.host != HOST) {
            return null
        }
        if (uri.path.orEmpty().startsWith("/creation/")) {
            return bundledAsset(uri)
        }
        if (!uri.path.orEmpty().startsWith("/asset/")) return null
        val token = uri.lastPathSegment ?: return notFound()
        val asset = assets[token] ?: return notFound()
        return runCatching {
            WebResourceResponse(
                asset.mimeType,
                null,
                200,
                "OK",
                mapOf(
                    "Access-Control-Allow-Origin" to "*",
                    "Cache-Control" to "no-store",
                ),
                files.openInput(asset.path).buffered(),
            )
        }.getOrElse { notFound() }
    }

    private fun bundledAsset(uri: Uri): WebResourceResponse {
        val path = uri.path.orEmpty().removePrefix("/")
        if (path.contains("..")) return notFound()
        return runCatching {
            WebResourceResponse(
                mimeType(path),
                if (path.endsWith(".html") || path.endsWith(".js") || path.endsWith(".css")) {
                    "utf-8"
                } else {
                    null
                },
                200,
                "OK",
                mapOf("Cache-Control" to "no-cache"),
                assetManager.open(path).buffered(),
            )
        }.getOrElse { notFound() }
    }

    private fun notFound(): WebResourceResponse = WebResourceResponse(
        "text/plain",
        "utf-8",
        404,
        "Not Found",
        mapOf("Access-Control-Allow-Origin" to "*"),
        "Not found".byteInputStream(),
    )

    private fun mimeType(path: String): String = when (
        path.substringBefore('?').substringAfterLast('.', "").lowercase()
    ) {
        "png" -> "image/png"
        "jpg", "jpeg" -> "image/jpeg"
        "webp" -> "image/webp"
        "svg" -> "image/svg+xml"
        "glb" -> "model/gltf-binary"
        "html" -> "text/html"
        "js" -> "text/javascript"
        "css" -> "text/css"
        "json" -> "application/json"
        "woff" -> "font/woff"
        "woff2" -> "font/woff2"
        else -> "application/octet-stream"
    }

    private data class Asset(val path: String, val mimeType: String)

    internal companion object {
        const val HOST = "sgt.local"
        const val ORIGIN = "https://sgt.local/asset"
        const val CREATION_ORIGIN = "https://sgt.local/creation"
    }
}
