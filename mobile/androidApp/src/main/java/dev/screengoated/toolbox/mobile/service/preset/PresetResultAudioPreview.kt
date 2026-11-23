package dev.screengoated.toolbox.mobile.service.preset

import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import dev.screengoated.toolbox.mobile.preset.sniffAudioMimeType
import java.io.ByteArrayInputStream
import java.util.UUID

internal data class AudioPreviewRange(val start: Int, val end: Int)

internal fun audioPreviewRange(header: String?, size: Int): AudioPreviewRange? {
    if (size <= 0) return null
    if (header == null) return AudioPreviewRange(0, size - 1)
    val match = Regex("bytes=(\\d*)-(\\d*)").matchEntire(header.trim()) ?: return null
    val first = match.groupValues[1]
    val last = match.groupValues[2]
    if (first.isEmpty() && last.isEmpty()) return null
    val start: Long
    val end: Long
    if (first.isEmpty()) {
        val count = last.toLongOrNull() ?: return null
        if (count <= 0) return null
        start = (size.toLong() - count).coerceAtLeast(0)
        end = size.toLong() - 1
    } else {
        start = first.toLongOrNull() ?: return null
        end = if (last.isEmpty()) size.toLong() - 1 else last.toLongOrNull() ?: return null
    }
    if (start >= size || end < start) return null
    return AudioPreviewRange(start.toInt(), end.coerceAtMost(size.toLong() - 1).toInt())
}

internal class ResultAudioPreview {
    private data class Media(val url: String, val bytes: ByteArray)
    @Volatile private var media: Media? = null

    fun replace(bytes: ByteArray?): String? {
        val next = bytes?.let { Media("$RESULT_TRUSTED_ORIGIN/input-audio/${UUID.randomUUID()}", it) }
        media = next
        return next?.url
    }

    internal fun accepts(url: String, method: String, mainFrame: Boolean): Boolean =
        !mainFrame && method == "GET" && media?.url == url

    fun intercept(request: WebResourceRequest): WebResourceResponse? {
        val current = media ?: return null
        if (request.isForMainFrame || request.method != "GET" || request.url.toString() != current.url || media !== current) return null
        val rangeHeader = request.requestHeaders.entries.firstOrNull { it.key.equals("Range", true) }?.value
        val range = audioPreviewRange(rangeHeader, current.bytes.size)
            ?: return WebResourceResponse("text/plain", "utf-8", 416, "Range Not Satisfiable", mapOf("Content-Range" to "bytes */${current.bytes.size}"), ByteArrayInputStream(ByteArray(0)))
        val size = range.end - range.start + 1
        val headers = mutableMapOf("Accept-Ranges" to "bytes", "Content-Length" to size.toString(), "Cache-Control" to "no-store")
        if (rangeHeader != null) headers["Content-Range"] = "bytes ${range.start}-${range.end}/${current.bytes.size}"
        return WebResourceResponse(
            sniffAudioMimeType(current.bytes), null, if (rangeHeader == null) 200 else 206,
            if (rangeHeader == null) "OK" else "Partial Content", headers,
            ByteArrayInputStream(current.bytes, range.start, size),
        )
    }
}
