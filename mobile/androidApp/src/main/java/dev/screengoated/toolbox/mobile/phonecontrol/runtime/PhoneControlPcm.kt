package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import java.util.Base64

internal fun decodePhoneControlPcm24k(mimeType: String?, data: String): ByteArray? {
    val normalized = mimeType.orEmpty().lowercase()
    if (!normalized.startsWith("audio/pcm") ||
        "rate=" in normalized && "rate=24000" !in normalized
    ) {
        Log.w("SGTPhoneControlTurn", "ignored_audio_part mime=${mimeType.orEmpty().take(80)}")
        return null
    }
    val bytes = runCatching { Base64.getDecoder().decode(data) }.getOrNull()
    if (bytes == null || bytes.isEmpty() || bytes.size % 2 != 0) {
        Log.w("SGTPhoneControlTurn", "ignored_invalid_pcm bytes=${bytes?.size ?: 0}")
        return null
    }
    return bytes
}
