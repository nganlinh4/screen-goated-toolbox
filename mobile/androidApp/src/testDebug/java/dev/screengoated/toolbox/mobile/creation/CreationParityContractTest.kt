package dev.screengoated.toolbox.mobile.creation

import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.double
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CreationParityContractTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun `image to 3d Android contract matches shared fixture`() {
        val fixture = fixture("image-to-3d")
        val limits = fixture["limits"]!!.jsonObject
        val defaults = fixture["defaults"]!!.jsonObject
        val names = fixture["names"]!!.jsonObject
        val presentation = fixture["presentation"]!!.jsonObject
        val lifecycle = fixture["hostLifecycle"]!!.jsonObject

        assertEquals(CreationContract.MINIMUM_POLYCOUNT, limits.int("minimumPolycount"))
        assertEquals(CreationContract.MAXIMUM_POLYCOUNT, limits.int("maximumPolycount"))
        assertEquals(CreationContract.MAXIMUM_PARALLEL_JOBS, limits.int("maximumParallelJobs"))
        assertEquals(
            CreationContract.MAXIMUM_CONCURRENT_PREPARATIONS,
            limits.int("maximumConcurrentPreparations"),
        )
        assertEquals(
            CreationContract.MINIMUM_PREPARATION_INTERVAL_SECONDS,
            limits.int("minimumPreparationIntervalSeconds"),
        )
        assertEquals(CreationContract.IMAGE_TO_3D_WORKSPACES, limits.int("preparedWorkspaces"))
        assertEquals(CreationContract.DEFAULT_POLYCOUNT, defaults.int("polycount"))
        assertTrue(presentation.boolean("sharedIconCatalog"))
        assertTrue(presentation.boolean("unchangedPollPreservesQueueDom"))
        assertTrue(presentation.boolean("hoveredSelectionTargetSurvivesPolling"))
        assertTrue(lifecycle.boolean("closeCancelsToolJobs"))
        assertTrue(lifecycle.boolean("closeDestroysWebSurface"))
        assertEquals(names.string("en"), MobileLocaleText.forLanguage("en").appImageTo3dTitle)
        assertEquals(names.string("ko"), MobileLocaleText.forLanguage("ko").appImageTo3dTitle)
        assertEquals(names.string("vi"), MobileLocaleText.forLanguage("vi").appImageTo3dTitle)
    }

    @Test
    fun `image to SVG Android contract matches shared fixture`() {
        val fixture = fixture("image-to-svg")
        val limits = fixture["limits"]!!.jsonObject
        val models = fixture["models"]!!.jsonObject
        val presentation = fixture["presentation"]!!.jsonObject
        val lifecycle = fixture["hostLifecycle"]!!.jsonObject

        assertEquals(CreationContract.MAXIMUM_PARALLEL_JOBS, limits.int("maximumParallelJobs"))
        assertEquals(
            CreationContract.MAXIMUM_CONCURRENT_PREPARATIONS,
            limits.int("maximumConcurrentPreparations"),
        )
        assertEquals(
            CreationContract.MINIMUM_PREPARATION_INTERVAL_SECONDS,
            limits.int("minimumPreparationIntervalSeconds"),
        )
        assertEquals(CreationContract.IMAGE_TO_SVG_WORKSPACES, limits.int("preparedWorkspaces"))
        assertEquals(setOf("simple", "detail"), models.keys)
        assertTrue(models.getValue("simple").jsonObject.boolean("selectable"))
        assertTrue(models.getValue("detail").jsonObject.boolean("selectable"))
        assertTrue(presentation.boolean("sharedIconCatalog"))
        assertTrue(presentation.boolean("unchangedPollPreservesQueueDom"))
        assertTrue(presentation.boolean("hoveredSelectionTargetSurvivesPolling"))
        assertTrue(lifecycle.boolean("closeCancelsToolJobs"))
        assertTrue(lifecycle.boolean("closeDestroysWebSurface"))
    }

    @Test
    fun `image creator Android contract matches shared fixture`() {
        val fixture = fixture("image-creation-editing")
        val prompt = fixture["request"]!!.jsonObject["prompt"]!!.jsonObject
        val references = fixture["request"]!!.jsonObject["references"]!!.jsonObject
        val locales = fixture["locales"]!!.jsonObject
        val presentation = fixture["presentation"]!!.jsonObject
        val surface = fixture["androidSurface"]!!.jsonObject
        val behavior = fixture["behavior"]!!.jsonObject
        val copyPolicy = fixture["publicCopyPolicy"]!!.jsonObject

        assertEquals("image", fixture.string("tool"))
        assertEquals(CreationContract.IMAGE_CREATOR_OPERATION, fixture.string("operation"))
        assertEquals(
            CreationContract.IMAGE_CREATOR_MAXIMUM_PARALLEL_JOBS,
            fixture.int("maximumParallelJobs"),
        )
        assertEquals(
            CreationContract.IMAGE_CREATOR_WORKSPACES,
            fixture.int("preparedWorkspaces"),
        )
        assertEquals(
            CreationContract.MAXIMUM_CONCURRENT_PREPARATIONS,
            fixture.int("maximumConcurrentPreparations"),
        )
        assertEquals(
            CreationContract.IMAGE_CREATOR_MAXIMUM_PROMPT_CHARACTERS,
            prompt.int("maximumCharacters"),
        )
        assertEquals(0, references.int("minimum"))
        assertEquals(
            CreationContract.IMAGE_CREATOR_MAXIMUM_REFERENCE_IMAGES,
            references.int("maximum"),
        )
        assertEquals("Google Sans Flex", presentation.string("fontFamily"))
        assertEquals(100, presentation.int("fontRoundedAxis"))
        assertEquals("Material Symbols Rounded", presentation.string("iconFamily"))
        assertEquals(1, presentation.int("iconFill"))
        assertFalse(presentation.boolean("appSpecificTheme"))
        assertTrue(presentation.boolean("sharedIconCatalog"))
        assertTrue(presentation.boolean("unchangedPollPreservesQueueDom"))
        assertTrue(presentation.boolean("hoveredSelectionTargetSurvivesPolling"))
        assertTrue(presentation.boolean("focusedInputSurvivesStatusPolling"))
        assertTrue(presentation.boolean("imeCompositionSurvivesStatusPolling"))
        val estimatedProgress = presentation["estimatedProgress"]!!.jsonObject
        assertTrue(estimatedProgress.boolean("usesRuntimeEstimate"))
        assertTrue(estimatedProgress.boolean("usesElapsedTimeCurve"))
        assertTrue(estimatedProgress.boolean("monotonic"))
        assertEquals(0.94, estimatedProgress.double("maximumBeforeCompletion"), 0.0)
        assertTrue(estimatedProgress.boolean("showsLocalizedEta"))
        assertEquals(
            CreationContract.IMAGE_CREATOR_WORKSPACES,
            surface.int("isolatedWorkers"),
        )
        assertFalse(surface.boolean("implementationDetailsVisible"))
        assertEquals("feature_only", copyPolicy.string("vocabulary"))
        assertFalse(copyPolicy.boolean("implementationDetailsVisible"))
        assertFalse(copyPolicy.boolean("rawImplementationErrorsVisible"))
        assertTrue(copyPolicy.boolean("referenceUploadCopyRequiresReferences"))
        assertTrue(behavior.boolean("cancellationIsMonotonic"))
        assertTrue(behavior.boolean("lateSuccessCannotPublishAfterCancellation"))
        assertTrue(behavior.boolean("acceptedRequestIsNotRepeatedDuringRecovery"))
        assertTrue(behavior.boolean("retryCreatesNewJob"))
        assertTrue(behavior.boolean("retryPreservesPreviousResult"))
        assertTrue(behavior.boolean("closingUiCancelsToolJobs"))
        assertTrue(behavior.boolean("closingUiTerminatesTrackedProcessTrees"))
        assertTrue(behavior.boolean("closingUiDestroysWebSurface"))
        assertTrue(behavior.boolean("sharedPreparationSurvivesMiniAppClose"))
        assertTrue(behavior.boolean("failureRemainsBoundToJob"))
        assertEquals(locales.string("en"), MobileLocaleText.forLanguage("en").appImageCreatorTitle)
        assertEquals(locales.string("ko"), MobileLocaleText.forLanguage("ko").appImageCreatorTitle)
        assertEquals(locales.string("vi"), MobileLocaleText.forLanguage("vi").appImageCreatorTitle)
    }

    private fun fixture(tool: String) = json.parseToJsonElement(
        File(repoRoot(), "parity-fixtures/$tool/state-contract.json").readText(),
    ).jsonObject

    private fun repoRoot(): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = directory.parentFile?.canonicalFile ?: error("Could not find repository root")
        }
        return directory
    }
}

private fun kotlinx.serialization.json.JsonObject.int(key: String): Int =
    this[key]!!.jsonPrimitive.int

private fun kotlinx.serialization.json.JsonObject.string(key: String): String =
    this[key]!!.jsonPrimitive.content

private fun kotlinx.serialization.json.JsonObject.boolean(key: String): Boolean =
    this[key]!!.jsonPrimitive.boolean

private fun kotlinx.serialization.json.JsonObject.double(key: String): Double =
    this[key]!!.jsonPrimitive.double
