package dev.screengoated.toolbox.mobile.service.tts

/** Route and volume fields of a playback snapshot; each one is an AudioManager binder read. */
internal data class PlaybackRouteInfo(
    val audioMode: Int?,
    val communicationDevice: String?,
    val routedDevice: String?,
    val voiceCallVolume: Int?,
    val voiceCallMaxVolume: Int?,
    val musicVolume: Int?,
    val musicMaxVolume: Int?,
)

/**
 * Playback snapshots are polled per audio frame by the live runtimes. The route
 * fields change only on user or system action, so they are re-read at most
 * once per [ttlMs], and immediately after the player changes routing itself.
 */
internal class PlaybackRouteCache(
    private val ttlMs: Long,
    private val read: () -> PlaybackRouteInfo,
) {
    private var cached: PlaybackRouteInfo? = null
    private var readAtMs = 0L

    @Synchronized
    fun get(nowMs: Long): PlaybackRouteInfo {
        val current = cached
        if (current != null && nowMs - readAtMs in 0 until ttlMs) return current
        return read().also {
            cached = it
            readAtMs = nowMs
        }
    }

    @Synchronized
    fun invalidate() {
        cached = null
    }
}

/** Decodes little-endian PCM16 bytes; a trailing odd byte is ignored. */
internal fun decodePcm16Le(bytes: ByteArray): ShortArray = ShortArray(bytes.size / 2) { index ->
    val offset = index * 2
    ((bytes[offset + 1].toInt() shl 8) or (bytes[offset].toInt() and 0xFF)).toShort()
}
