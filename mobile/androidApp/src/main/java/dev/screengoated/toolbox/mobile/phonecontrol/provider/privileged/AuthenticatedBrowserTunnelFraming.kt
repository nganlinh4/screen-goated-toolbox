package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import java.io.ByteArrayOutputStream
import java.io.InputStream
import java.io.OutputStream
import java.nio.charset.StandardCharsets
import java.util.concurrent.atomic.AtomicBoolean

/** The one authenticated request a tunnel connection may forward: stripped header plus its declared body size. */
internal class ForwardedTunnelRequest(
    val header: ByteArray,
    val bodyLength: Long,
)

/**
 * Body size of an already-validated request header. Chunked or ambiguous
 * framing is refused, because the tunnel must know exactly where the single
 * authenticated request ends.
 */
internal fun tunnelRequestBodyLength(header: ByteArray): Long? {
    val fields = header.toString(StandardCharsets.ISO_8859_1)
        .removeSuffix("\r\n\r\n")
        .split("\r\n")
        .drop(1)
    fun named(name: String) = fields.filter { it.substringBefore(':', "").trim().equals(name, ignoreCase = true) }
    if (named("Transfer-Encoding").isNotEmpty()) return null
    val lengths = named("Content-Length")
    if (lengths.isEmpty()) return 0L
    val value = lengths.singleOrNull()?.substringAfter(':')?.trim() ?: return null
    if (value.isEmpty() || value.length > MAX_BODY_DIGITS || !value.all { it in '0'..'9' }) return null
    return value.toLong().takeIf { it <= MAX_TUNNEL_BODY_BYTES }
}

/**
 * Forwards the authenticated request's body, then lets further client bytes
 * through only once Chrome has switched the connection to a WebSocket. Any
 * other byte starts a second HTTP request whose headers were never
 * authenticated or stripped, so the connection ends instead of forwarding it.
 */
internal fun forwardTunnelClientStream(
    input: InputStream,
    output: OutputStream,
    bodyLength: Long,
    upgraded: AtomicBoolean,
) {
    val buffer = ByteArray(TUNNEL_COPY_BUFFER_BYTES)
    var remaining = bodyLength
    while (remaining > 0L) {
        val count = input.read(buffer, 0, minOf(buffer.size.toLong(), remaining).toInt())
        if (count < 0) return
        output.write(buffer, 0, count)
        remaining -= count
    }
    output.flush()
    val next = input.read()
    // A WebSocket client sends nothing before it has received the 101 that sets this flag.
    if (next < 0 || !upgraded.get()) return
    output.write(next)
    output.flush()
    copyTunnelStreamFlushing(input, output)
}

/**
 * Relays until end of stream, flushing after every chunk. A buffered upstream
 * (the ADB stream) would otherwise hold WebSocket frames until its buffer fills.
 */
internal fun copyTunnelStreamFlushing(input: InputStream, output: OutputStream) {
    val buffer = ByteArray(TUNNEL_COPY_BUFFER_BYTES)
    while (true) {
        val count = input.read(buffer)
        if (count < 0) return
        if (count == 0) continue
        output.write(buffer, 0, count)
        output.flush()
    }
}

/** Reads the upstream status line, bounded, so the relay can see a WebSocket upgrade before passing it on. */
internal fun readTunnelStatusLine(input: InputStream): ByteArray {
    val line = ByteArrayOutputStream()
    while (line.size() < MAX_STATUS_LINE_BYTES) {
        val next = input.read()
        if (next < 0) break
        line.write(next)
        if (next == '\n'.code) break
    }
    return line.toByteArray()
}

internal fun isSwitchingProtocols(statusLine: ByteArray): Boolean {
    val parts = statusLine.toString(StandardCharsets.ISO_8859_1).trimEnd().split(' ')
    return parts.size >= 2 && parts[0].startsWith("HTTP/1.") && parts[1] == "101"
}

internal const val TUNNEL_COPY_BUFFER_BYTES = 16 * 1_024
private const val MAX_TUNNEL_BODY_BYTES = 1_024L * 1_024L
private const val MAX_BODY_DIGITS = 7
private const val MAX_STATUS_LINE_BYTES = 1_024
