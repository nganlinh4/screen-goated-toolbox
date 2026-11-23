package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.nio.charset.CharacterCodingException
import java.nio.charset.CodingErrorAction
import java.nio.charset.StandardCharsets
import java.util.Base64
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal const val COMMAND_RAW_PREFIX_BYTES = 1024

internal data class CommandOutputSnapshot(
    val bytes: ByteArray,
    val observedBytes: Long,
    val truncated: Boolean,
) {
    fun evidence(sourceComplete: Boolean): JsonObject {
        val lossy = try {
            StandardCharsets.UTF_8.newDecoder()
                .onMalformedInput(CodingErrorAction.REPORT)
                .onUnmappableCharacter(CodingErrorAction.REPORT)
                .decode(ByteBuffer.wrap(bytes))
            false
        } catch (_: CharacterCodingException) {
            true
        }
        return buildJsonObject {
            put("output", String(bytes, StandardCharsets.UTF_8))
            put("output_truncated", truncated)
            put("output_bytes", observedBytes)
            put("output_encoding", "unknown_or_mixed")
            put("output_text_exact", false)
            put("output_lossy", lossy)
            put("output_bytes_base64", Base64.getEncoder().encodeToString(bytes.copyOf(minOf(bytes.size, COMMAND_RAW_PREFIX_BYTES))))
            put("output_bytes_available", true)
            put("output_bytes_complete", sourceComplete && !truncated && bytes.size <= COMMAND_RAW_PREFIX_BYTES)
            put("output_complete", sourceComplete && !truncated)
            put("output_stream", "merged_stdout_stderr")
        }
    }
}

/** One synchronized snapshot prevents mixing counts from a still-running reader. */
internal class BoundedCommandOutput(private val limit: Int) {
    private val bytes = ByteArrayOutputStream()
    private var totalBytes = 0L
    private var truncated = false

    @Synchronized
    fun write(buffer: ByteArray, offset: Int, length: Int) {
        totalBytes = if (Long.MAX_VALUE - totalBytes < length) Long.MAX_VALUE else totalBytes + length
        val remaining = limit - bytes.size()
        if (remaining > 0) bytes.write(buffer, offset, minOf(length, remaining))
        if (length > remaining) truncated = true
    }

    @Synchronized
    fun snapshot(): CommandOutputSnapshot =
        CommandOutputSnapshot(bytes.toByteArray(), totalBytes, truncated)
}
