package dev.screengoated.toolbox.mobile.creation

import java.io.File
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

class CreationRefinementSettingsTest {
    private val json = Json { encodeDefaults = true }

    @Test
    fun `saved refinement keeps generation level separate from its requested action`() {
        val root = generateSequence(File(requireNotNull(System.getProperty("user.dir"))).absoluteFile) {
            it.parentFile
        }.first { File(it, "parity-fixtures").isDirectory }
        val fixture = json.parseToJsonElement(
            File(root, "parity-fixtures/image-to-3d/refinement-settings.json").readText(),
        ).jsonObject
        for (case in fixture.getValue("cases").jsonArray) {
            val value = case.jsonObject
            val generation = value.getValue("generationLevel").jsonPrimitive.content
            val requested = value.getValue("requestedLevel").jsonPrimitive.contentOrNull
            val kind = if (requested == null) "materials" else "separate_$requested"
            val executionLevel = creationRefinementSegmentationLevel(kind, generation)
            assertEquals(requested ?: generation, executionLevel)
            val record = CreationJournalRecord(
                ownerId = "owner",
                request = request(executionLevel),
                status = CreationJobStatus(stage = "refining", progressText = "Creating", segmentationLevel = generation),
                startedAtMs = 1L,
            )
            val saved = json.decodeFromString<CreationJournalRecord>(json.encodeToString(record))
            assertEquals(generation, creationContinuationSegmentationLevel(saved.status, saved.request))
        }
    }

    @Test
    fun `older records without status settings retain their recorded request level`() {
        for (level in listOf("simple", "balanced", "detailed")) {
            assertEquals(level, creationContinuationSegmentationLevel(
                CreationJobStatus(stage = "done", progressText = "Ready"), request(level),
            ))
        }
    }

    private fun request(level: String) = CreationWorkerRequest(
        jobId = "job", tool = "3d", operation = "refine", imagePath = "source.png",
        outputPath = "result.glb", outputName = "result.glb", segmentationLevel = level,
    )
}
