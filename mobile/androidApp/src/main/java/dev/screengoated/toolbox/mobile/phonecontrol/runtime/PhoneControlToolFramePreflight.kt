package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveFunctionCall
import java.nio.charset.StandardCharsets

internal enum class PhoneControlToolFrameRejection {
    TOO_MANY_CALLS,
    ID_TOO_LARGE,
    NAME_TOO_LARGE,
    ARGUMENTS_TOO_LARGE,
    FRAME_TOO_LARGE,
}

/** Structural input bounds only; tool meaning remains entirely model-owned. */
internal object PhoneControlToolFramePreflight {
    const val MAXIMUM_CALLS = 33
    const val MAXIMUM_ID_UTF8_BYTES = 1_024
    const val MAXIMUM_NAME_UTF8_BYTES = 1_024
    const val MAXIMUM_ARGUMENTS_UTF8_BYTES = 1024 * 1024
    const val MAXIMUM_FRAME_UTF8_BYTES = 2 * 1024 * 1024

    fun rejection(calls: List<GeminiLiveFunctionCall>): PhoneControlToolFrameRejection? {
        if (calls.size > MAXIMUM_CALLS) return PhoneControlToolFrameRejection.TOO_MANY_CALLS
        var frameBytes = 0L
        for (call in calls) {
            val idBytes = call.id.utf8Bytes()
            if (idBytes > MAXIMUM_ID_UTF8_BYTES) return PhoneControlToolFrameRejection.ID_TOO_LARGE
            val nameBytes = call.name.utf8Bytes()
            if (nameBytes > MAXIMUM_NAME_UTF8_BYTES) {
                return PhoneControlToolFrameRejection.NAME_TOO_LARGE
            }
            val argumentsBytes = call.args.toString().utf8Bytes()
            if (argumentsBytes > MAXIMUM_ARGUMENTS_UTF8_BYTES) {
                return PhoneControlToolFrameRejection.ARGUMENTS_TOO_LARGE
            }
            frameBytes += idBytes.toLong() + nameBytes + argumentsBytes
            if (frameBytes > MAXIMUM_FRAME_UTF8_BYTES) {
                return PhoneControlToolFrameRejection.FRAME_TOO_LARGE
            }
        }
        return null
    }

    private fun String.utf8Bytes(): Int = toByteArray(StandardCharsets.UTF_8).size
}
