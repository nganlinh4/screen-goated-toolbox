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
import org.junit.Assert.assertTrue
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
        assertEquals("depth_anything_3_relief", surface.stringAt("progressPreview"))
        assertEquals(DepthPreviewContract.INPUT_SIDE, surface.intAt("previewInputSide"))
        assertFalse(surface.booleanAt("previewBlocksGeneration"))
        assertFalse(surface.booleanAt("previewSetupVisible"))
        assertEquals(18, surface.intAt("preparationProgressMaximumPercent"))
        assertEquals("bounded_privacy_safe_journal", surface.stringAt("diagnostics"))
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
        assertEquals("depth_anything_3_six_bins", surface.stringAt("progressPreview"))
        assertEquals(DepthPreviewContract.INPUT_SIDE, surface.intAt("previewInputSide"))
        assertFalse(surface.booleanAt("previewBlocksGeneration"))
        assertFalse(surface.booleanAt("previewSetupVisible"))
        assertEquals(18, surface.intAt("preparationProgressMaximumPercent"))
        assertEquals("bounded_privacy_safe_journal", surface.stringAt("diagnostics"))
        assertFalse(surface.booleanAt("backgroundAutomationVisible"))
    }

    @Test
    fun `android depth preview uses the canonical windows model`() {
        val windowsSource = File(
            repoRoot(),
            "src/overlay/three_d_generator/depth_model.rs",
        ).readText()

        assertTrue(windowsSource.contains(DepthPreviewContract.MODEL_URL))
        assertTrue(windowsSource.replace("_", "").contains(DepthPreviewContract.MODEL_BYTES.toString()))
        assertTrue(windowsSource.contains(DepthPreviewContract.MODEL_SHA256))
        assertTrue(windowsSource.contains("const SIDE: u32 = ${DepthPreviewContract.INPUT_SIDE};"))
    }

    @Test
    fun `mailbox preparation uses patient capped retries`() {
        assertEquals(4, CreationContract.IMAGE_TO_3D_WORKSPACES)
        assertEquals(1, CreationContract.MAXIMUM_CONCURRENT_PREPARATIONS)
        assertEquals(5 * 60_000L, CreationPreparationCooldown.mailboxFailureBackoffMs(1))
        assertEquals(10 * 60_000L, CreationPreparationCooldown.mailboxFailureBackoffMs(2))
        assertEquals(15 * 60_000L, CreationPreparationCooldown.mailboxFailureBackoffMs(3))
        assertEquals(15 * 60_000L, CreationPreparationCooldown.mailboxFailureBackoffMs(20))
    }

    private fun loadFixture(path: String): JsonObject {
        return json.parseToJsonElement(File(repoRoot(), path).readText()).jsonObject
    }

    private fun repoRoot(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile
        }.firstOrNull { root -> File(root, "parity-fixtures").isDirectory }
            ?: error("Could not locate the repository from $workingDirectory")
    }

    private fun JsonObject.objectAt(key: String) = requireNotNull(this[key]).jsonObject
    private fun JsonObject.intAt(key: String) = requireNotNull(this[key]).jsonPrimitive.int
    private fun JsonObject.stringAt(key: String) = requireNotNull(this[key]).jsonPrimitive.content
    private fun JsonObject.booleanAt(key: String) = requireNotNull(this[key]).jsonPrimitive.boolean
}
