package dev.screengoated.toolbox.mobile.service.preset

import java.io.File
import java.io.RandomAccessFile
import java.nio.ByteBuffer
import java.nio.ByteOrder

internal enum class AudioAttachmentAppend { APPENDED, OMITTED, OMITTED_CONTINUING, LIMIT_REACHED, IGNORED }

/** Session-owned PCM spool; continuous overflow never returns an incomplete attachment. */
internal class StreamingAudioAttachment(
    directory: File,
    private val limit: Int = 16_000 * 60 * 10,
) {
    private val file = File.createTempFile("preset-audio-", ".pcm", directory)
    private var output: RandomAccessFile? = RandomAccessFile(file, "rw")
    private var count = 0
    private var limitReached = false
    private var fallbackToFinite = false
    @Volatile
    var omitted = false
        private set

    @Synchronized
    fun append(chunk: ShortArray, bounded: Boolean): AudioAttachmentAppend {
        if (omitted) return AudioAttachmentAppend.OMITTED_CONTINUING
        if (limitReached || output == null) return AudioAttachmentAppend.IGNORED
        if (chunk.size > limit - count) {
            if (bounded && !fallbackToFinite) {
                omitted = true
                closeAndDelete()
                return AudioAttachmentAppend.OMITTED
            }
            limitReached = true
            return AudioAttachmentAppend.LIMIT_REACHED
        }
        val bytes = ByteArray(chunk.size * 2)
        val buffer = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
        chunk.forEach { buffer.putShort(it) }
        output!!.write(bytes)
        count += chunk.size
        return AudioAttachmentAppend.APPENDED
    }

    /** Atomically prevents live overflow from discarding a still-recoverable fallback. */
    @Synchronized
    fun fallBackToFinite(): Boolean {
        if (omitted || output == null) return false
        fallbackToFinite = true
        return true
    }

    @Synchronized
    fun toWavBytes(): ByteArray {
        if (omitted) return byteArrayOf()
        val input = checkNotNull(output) { "Audio attachment is closed" }
        val dataSize = count * 2
        val bytes = ByteArray(44 + dataSize)
        val header = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
        header.put("RIFF".toByteArray(Charsets.US_ASCII)).putInt(36 + dataSize)
        header.put("WAVEfmt ".toByteArray(Charsets.US_ASCII)).putInt(16)
        header.putShort(1).putShort(1).putInt(16_000).putInt(32_000)
        header.putShort(2).putShort(16)
        header.put("data".toByteArray(Charsets.US_ASCII)).putInt(dataSize)
        input.seek(0)
        input.readFully(bytes, 44, dataSize)
        return bytes
    }

    @Synchronized
    fun clear() = closeAndDelete()

    private fun closeAndDelete() {
        try {
            output?.close()
        } finally {
            output = null
            file.delete()
        }
    }
}
