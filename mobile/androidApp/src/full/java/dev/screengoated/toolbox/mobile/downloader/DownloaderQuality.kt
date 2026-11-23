package dev.screengoated.toolbox.mobile.downloader

import org.json.JSONObject

internal const val DEFAULT_VIDEO_QUALITY = "1080p"
internal const val COMPLETED_QUALITY_PREFIX = "SGT_VIDEO_QUALITY "
internal const val COMPLETED_QUALITY_TEMPLATE = "after_move:SGT_VIDEO_QUALITY %(.{width,height})j"

internal fun qualityChoices(remembered: String?, available: List<String>): List<String> =
    (listOf(DEFAULT_VIDEO_QUALITY, "720p") + listOfNotNull(remembered) + available).distinct()

internal fun reportedQuality(record: String): Int? {
    val dimensions = runCatching { JSONObject(record) }.getOrNull() ?: return null
    val width = dimensions.optInt("width").takeIf { it > 0 } ?: return null
    val height = dimensions.optInt("height").takeIf { it > 0 } ?: return null
    return minOf(width, height)
}

internal fun completedQuality(output: String): Int? = output.lineSequence()
    .filter { it.startsWith(COMPLETED_QUALITY_PREFIX) }
    .map { reportedQuality(it.removePrefix(COMPLETED_QUALITY_PREFIX)) }
    .toSet()
    .singleOrNull()
