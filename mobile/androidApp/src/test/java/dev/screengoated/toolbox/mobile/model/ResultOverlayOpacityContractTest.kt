package dev.screengoated.toolbox.mobile.model

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

class ResultOverlayOpacityContractTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun defaultMatchesParityAndPreservesExplicitSavedValues() {
        val fixtureFile = findRepoRoot()
            .resolve("parity-fixtures/preset-system/result-overlay.json")
        val opacity = json.parseToJsonElement(fixtureFile.readText())
            .jsonObject["opacity"]!!
            .jsonObject

        assertEquals(
            DEFAULT_RESULT_OVERLAY_OPACITY_PERCENT,
            opacity["default_percent"]!!.jsonPrimitive.int,
        )
        assertEquals(
            MIN_RESULT_OVERLAY_OPACITY_PERCENT,
            opacity["minimum_percent"]!!.jsonPrimitive.int,
        )
        assertEquals(
            MAX_RESULT_OVERLAY_OPACITY_PERCENT,
            opacity["maximum_percent"]!!.jsonPrimitive.int,
        )
        assertEquals(DEFAULT_RESULT_OVERLAY_OPACITY_PERCENT, MobileUiPreferences().overlayOpacityPercent)
        assertEquals(
            85,
            json.decodeFromString<MobileUiPreferences>("""{"overlayOpacityPercent":85}""")
                .overlayOpacityPercent,
        )
        assertEquals(
            DEFAULT_RESULT_OVERLAY_OPACITY_PERCENT,
            json.decodeFromString<MobileUiPreferences>("{}").overlayOpacityPercent,
        )
    }

    private fun findRepoRoot(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile
        }.first { root ->
            root.resolve("parity-fixtures/preset-system/result-overlay.json").exists()
        }
    }
}
