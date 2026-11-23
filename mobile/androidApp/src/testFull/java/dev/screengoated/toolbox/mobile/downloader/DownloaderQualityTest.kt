package dev.screengoated.toolbox.mobile.downloader

import java.io.File
import kotlinx.serialization.json.Json
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test

class DownloaderQualityTest {
    @Test
    fun freshAndSavedPreferencesMatchTheSharedContract() {
        val fixture = qualityFixture()
        val preview = fixture.getJSONObject("preview")
        val available = preview.getJSONArray("available")
        val resolutions = List(available.length()) { available.getString(it) }

        assertEquals(fixture.getString("freshPreference"), decodeDownloaderSettings(Json, null).lastVideoFormat)
        assertEquals(null, decodeDownloaderSettings(Json, "{}").lastVideoFormat)
        assertEquals(null, decodeDownloaderSettings(Json, """{"lastVideoFormat":null}""").lastVideoFormat)
        val remembered = decodeDownloaderSettings(Json, """{"lastVideoFormat":"720p"}""").lastVideoFormat
        assertEquals(preview.getString("remembered"), remembered)
        assertEquals(listOf("1080p", "720p", "2160p", "480p"), qualityChoices(remembered, resolutions))
        assertEquals(preview.getString("withOrWithoutScan"), resolutionSort(remembered))
    }

    @Test
    fun completionReportsUseTheShortSideAndOmitUnknownOrMixedResolutions() {
        val cases = qualityFixture().getJSONArray("completed")
        for (index in 0 until cases.length()) {
            val case = cases.getJSONObject(index)
            val reports = case.getJSONArray("reports")
            val output = (0 until reports.length()).joinToString("\n") {
                COMPLETED_QUALITY_PREFIX + reports.getJSONObject(it).toString()
            }
            val expected = case.optInt("resolution").takeIf { it > 0 }
            assertEquals(expected, completedQuality(output))
        }
    }
}

private fun qualityFixture(): JSONObject {
    val fixture = generateSequence(File(requireNotNull(System.getProperty("user.dir")))) { it.parentFile }
        .map { it.resolve("parity-fixtures/video-downloader/quality-preferences.json") }
        .first { it.isFile }
    return JSONObject(fixture.readText())
}
