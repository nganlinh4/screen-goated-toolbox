package dev.screengoated.toolbox.mobile.creation

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class CreationStateContractTest {
    private val json = Json { ignoreUnknownKeys = false }

    @Test
    fun `image to 3d native shell preserves canonical limits`() {
        val fixture = loadFixture("parity-fixtures/image-to-3d/state-contract.json")
        val defaults = fixture.objectAt("defaults")
        val limits = fixture.objectAt("limits")
        val surface = fixture.objectAt("androidSurface")

        assertEquals(CreationContract.DEFAULT_POLYCOUNT, defaults.intAt("polycount"))
        assertFalse(defaults.booleanAt("autoSegment"))
        assertEquals(CreationContract.MINIMUM_POLYCOUNT, limits.intAt("minimumPolycount"))
        assertEquals(CreationContract.MAXIMUM_POLYCOUNT, limits.intAt("maximumPolycount"))
        assertEquals(CreationContract.MAXIMUM_PARALLEL_JOBS, limits.intAt("maximumParallelJobs"))
        assertEquals("native_compose_m3e", surface.stringAt("shell"))
        assertEquals("sceneview_filament", surface.stringAt("resultRenderer"))
        assertFalse(surface.booleanAt("backgroundAutomationVisible"))
    }

    @Test
    fun `image to svg native shell preserves canonical limits`() {
        val fixture = loadFixture("parity-fixtures/image-to-svg/state-contract.json")
        val limits = fixture.objectAt("limits")
        val models = fixture.objectAt("models")
        val surface = fixture.objectAt("androidSurface")

        assertEquals(CreationContract.MAXIMUM_PARALLEL_JOBS, limits.intAt("maximumParallelJobs"))
        assertEquals(2, models.objectAt("simple").intAt("creditCost"))
        assertEquals(4, models.objectAt("detail").intAt("creditCost"))
        assertEquals("native_compose_m3e", surface.stringAt("shell"))
        assertEquals("sandboxed_svg_document", surface.stringAt("resultRenderer"))
        assertFalse(surface.booleanAt("backgroundAutomationVisible"))
    }

    private fun loadFixture(path: String): JsonObject {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        val repoRoot = generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile
        }.firstOrNull { root -> File(root, path).isFile }
            ?: error("Could not locate $path from $workingDirectory")
        return json.parseToJsonElement(File(repoRoot, path).readText()).jsonObject
    }

    private fun JsonObject.objectAt(key: String) = requireNotNull(this[key]).jsonObject
    private fun JsonObject.intAt(key: String) = requireNotNull(this[key]).jsonPrimitive.int
    private fun JsonObject.stringAt(key: String) = requireNotNull(this[key]).jsonPrimitive.content
    private fun JsonObject.booleanAt(key: String) = requireNotNull(this[key]).jsonPrimitive.boolean
}
