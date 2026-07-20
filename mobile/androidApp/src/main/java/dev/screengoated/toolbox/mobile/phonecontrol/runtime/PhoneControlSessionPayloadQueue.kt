package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import java.nio.charset.StandardCharsets
import java.util.ArrayDeque

internal data class PhoneControlPayloadQueueSnapshot(
    val count: Int,
    val utf8Bytes: Int,
    val overflowed: Boolean,
)

internal enum class PhoneControlOutboundKind(val contractValue: String) {
    TOOL_RESPONSE("tool_response"),
    TOOL_SCREEN_EVIDENCE("tool_screen_evidence"),
    AMBIENT_SCREEN("ambient_screen"),
    MICROPHONE_AUDIO("microphone_audio"),
}

internal data class PhoneControlQueuedPayload(
    val payload: String,
    val kind: PhoneControlOutboundKind,
    internal val utf8Bytes: Int,
)

/** Bounded logical-session FIFO. A rejected send leaves exactly one head in place. */
internal class PhoneControlSessionPayloadQueue(
    private val maximumCount: Int = MAXIMUM_COUNT,
    private val maximumUtf8Bytes: Int = MAXIMUM_UTF8_BYTES,
    private val maximumPayloadUtf8Bytes: Int = MAXIMUM_PAYLOAD_UTF8_BYTES,
) {
    private val lock = Any()
    private val queued = ArrayDeque<PhoneControlQueuedPayload>(maximumCount)
    private var queuedUtf8Bytes = 0
    private var overflowed = false
    private var closed = false

    init {
        require(maximumCount > 0)
        require(maximumUtf8Bytes > 0)
        require(maximumPayloadUtf8Bytes in 1..maximumUtf8Bytes)
    }

    fun offer(payload: String, kind: PhoneControlOutboundKind): Boolean = synchronized(lock) {
        if (closed || overflowed) return@synchronized false
        val payloadBytes = payload.toByteArray(StandardCharsets.UTF_8).size
        if (payloadBytes > maximumPayloadUtf8Bytes ||
            queued.size >= maximumCount ||
            queuedUtf8Bytes > maximumUtf8Bytes - payloadBytes
        ) {
            overflowed = true
            return@synchronized false
        }
        queued.addLast(PhoneControlQueuedPayload(payload, kind, payloadBytes))
        queuedUtf8Bytes += payloadBytes
        true
    }

    fun next(): PhoneControlQueuedPayload? = synchronized(lock) { queued.peekFirst() }

    fun markSent(payload: PhoneControlQueuedPayload) = synchronized(lock) {
        val head = queued.peekFirst() ?: error("no session payload is pending")
        check(head === payload) { "only the FIFO head can be acknowledged" }
        queued.removeFirst()
        queuedUtf8Bytes -= head.utf8Bytes
    }

    fun prepareReconnect(resumptionHandle: String?): Boolean = synchronized(lock) {
        if (PhoneControlResumptionPolicy.usableHandle(resumptionHandle) != null) return@synchronized true
        clear()
        false
    }

    fun abandonSession() = synchronized(lock) {
        clear()
    }

    fun snapshot(): PhoneControlPayloadQueueSnapshot = synchronized(lock) {
        PhoneControlPayloadQueueSnapshot(queued.size, queuedUtf8Bytes, overflowed)
    }

    fun close() = synchronized(lock) {
        clear()
        closed = true
    }

    private fun clear() {
        queued.clear()
        queuedUtf8Bytes = 0
        overflowed = false
    }

    internal companion object {
        // One evidence payload, one owner receipt, and all 32 held rejections.
        const val MAXIMUM_COUNT = 34
        const val MAXIMUM_UTF8_BYTES = 48 * 1024 * 1024
        const val MAXIMUM_PAYLOAD_UTF8_BYTES = 32 * 1024 * 1024
    }
}
