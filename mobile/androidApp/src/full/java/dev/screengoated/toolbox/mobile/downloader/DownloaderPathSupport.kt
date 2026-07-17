package dev.screengoated.toolbox.mobile.downloader

import java.nio.charset.CharacterCodingException

internal fun downloadTreePathToFilesystemPath(encodedPath: String?): String? {
    val markerIndex = encodedPath?.indexOf("/tree/") ?: return null
    if (markerIndex < 0) return null
    val rawTreePath = encodedPath.substring(markerIndex + "/tree/".length)
    val decodedTreePath = percentDecodePath(rawTreePath) ?: return null
    val separator = decodedTreePath.indexOf(':')
    if (separator < 0) return null

    val volume = decodedTreePath.substring(0, separator)
    if (volume != "primary") return null

    val relativePath = decodedTreePath.substring(separator + 1).trim('/')
    return buildString {
        append("/storage/emulated/0")
        if (relativePath.isNotBlank()) {
            append('/')
            append(relativePath)
        }
    }
}

private fun percentDecodePath(value: String): String? {
    val bytes = ByteArray(value.length)
    val output = StringBuilder(value.length)
    var byteCount = 0
    var index = 0

    fun flushBytes(): Boolean {
        if (byteCount == 0) return true
        val decoded = try {
            bytes.decodeToString(endIndex = byteCount)
        } catch (_: CharacterCodingException) {
            return false
        }
        output.append(decoded)
        byteCount = 0
        return true
    }

    while (index < value.length) {
        val char = value[index]
        if (char == '%') {
            if (index + 2 >= value.length) return null
            val high = value[index + 1].digitToIntOrNull(16) ?: return null
            val low = value[index + 2].digitToIntOrNull(16) ?: return null
            bytes[byteCount++] = ((high shl 4) + low).toByte()
            index += 3
        } else {
            if (!flushBytes()) return null
            output.append(char)
            index += 1
        }
    }

    return if (flushBytes()) output.toString() else null
}
