package dev.screengoated.toolbox.mobile.phonecontrol.provider.detector

import dev.screengoated.toolbox.mobile.preset.GeneratedPresetModelCatalogData
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths

class UiDetectorTargetSelectorTest {
    @Test
    fun generatedGroundingChainMatchesSharedFixture() {
        val fixture = Json.parseToJsonElement(
            Files.readAllBytes(fixturePath()).decodeToString(),
        ).jsonObject
        val expected = fixture.getValue("grounding").jsonObject
            .getValue("models").jsonArray
            .map { it.jsonPrimitive.content }

        assertEquals(expected, GeneratedPresetModelCatalogData.computerControlGroundingModelChain)
        assertEquals(expected, LOCATOR_MODEL_IDS)
    }

    @Test
    fun locatorChainFallsBackAndReportsTheSuccessfulModel() = runTest {
        val attempted = mutableListOf<String>()
        val result = runLocatorModelChain(
            modelIds = listOf("primary", "fallback"),
            execute = { model ->
                attempted += model
                if (model == "primary") Result.failure(IllegalStateException("quota"))
                else Result.success("valid")
            },
            parse = { _, model -> model },
            shouldAdvance = { false },
            requestFailure = { "failed" },
        )

        assertEquals(listOf("primary", "fallback"), attempted)
        assertEquals("fallback", result)
    }

    @Test
    fun terminalGroundingResultDoesNotConsultFallback() = runTest {
        val attempted = mutableListOf<String>()
        val result = runLocatorModelChain(
            modelIds = listOf("primary", "fallback"),
            execute = { model ->
                attempted += model
                Result.success("not-visible")
            },
            parse = { response, _ -> response },
            shouldAdvance = { false },
            requestFailure = { "failed" },
        )

        assertEquals(listOf("primary"), attempted)
        assertEquals("not-visible", result)
    }

    @Test
    fun malformedGroundingResultConsultsFallback() = runTest {
        val attempted = mutableListOf<String>()
        val result = runLocatorModelChain(
            modelIds = listOf("primary", "fallback"),
            execute = { model ->
                attempted += model
                Result.success(if (model == "primary") "malformed" else "valid")
            },
            parse = { response, _ -> response },
            shouldAdvance = { it == "malformed" },
            requestFailure = { "failed" },
        )

        assertEquals(listOf("primary", "fallback"), attempted)
        assertEquals("valid", result)
    }

    @Test
    fun parsesOneAllowedCurrentMarkDespiteNonJsonPreamble() {
        val result = parseUiDetectorTargetSelection(
            response = "<think>hidden</think>\n{\"mark\":7,\"confidence\":91,\"what\":\"yellow icon\"}",
            allowedMarks = setOf(3, 7, 9),
            modelId = "locator",
        )

        val success = result as UiDetectorTargetSelection.Success
        assertEquals(7, success.mark)
        assertEquals(91, success.confidence)
        assertEquals("yellow icon", success.what)
        assertEquals("locator", success.modelId)
    }

    @Test
    fun rejectsAWellFormedMarkOutsideTheCapturedFrame() {
        val result = parseUiDetectorTargetSelection(
            response = "{\"mark\":8,\"confidence\":100,\"what\":\"button\"}",
            allowedMarks = setOf(1, 2),
            modelId = "locator",
        )

        val failure = result as UiDetectorTargetSelection.Failure
        assertEquals("vision_grounding_invalid", failure.code)
        assertTrue(failure.retryable)
    }

    @Test
    fun visibleTargetAbsenceIsAProvenSelectionFailure() {
        val result = parseUiDetectorTargetSelection(
            response = "{\"error\":\"not visible\"}",
            allowedMarks = setOf(1),
            modelId = "locator",
        )

        val failure = result as UiDetectorTargetSelection.Failure
        assertEquals("target_not_found", failure.code)
        assertTrue(!failure.retryable)
    }

    @Test
    fun freshCrosshairMustMatchWithCanonicalConfidence() {
        val accepted = parseUiDetectorTargetVerification(
            "{\"matches\":true,\"confidence\":70,\"what\":\"target center\"}",
            "locator",
        )
        val rejected = parseUiDetectorTargetVerification(
            "{\"matches\":true,\"confidence\":69,\"what\":\"near target\"}",
            "locator",
        )

        assertEquals(70, (accepted as UiDetectorTargetVerification.Success).confidence)
        assertEquals(
            "vision_verification_rejected",
            (rejected as UiDetectorTargetVerification.Failure).code,
        )
        assertTrue(rejected.freshObservationRequired)
    }

    @Test
    fun parsesTwoDistinctDragEndpointsFromOneCurrentMarkSet() {
        val result = parseUiDetectorDragSelection(
            response = """{"from_mark":2,"from_confidence":88,"from_what":"handle","to_mark":9,"to_confidence":91,"to_what":"destination"}""",
            allowedMarks = setOf(2, 7, 9),
            modelId = "locator",
        )

        val success = result as UiDetectorDragSelection.Success
        assertEquals(2, success.from.mark)
        assertEquals(9, success.to.mark)
        assertEquals("handle", success.from.what)
        assertEquals("destination", success.to.what)
    }

    @Test
    fun rejectsDragEndpointsThatCollapseToOneDetectorAnchor() {
        val result = parseUiDetectorDragSelection(
            response = """{"from_mark":7,"from_confidence":99,"to_mark":7,"to_confidence":99}""",
            allowedMarks = setOf(7, 9),
            modelId = "locator",
        )

        val failure = result as UiDetectorDragSelection.Failure
        assertEquals("ambiguous_target", failure.code)
        assertTrue(failure.retryable)
    }

    private fun fixturePath(): Path {
        val candidates = listOf(
            Paths.get("..", "parity-fixtures", "phone-control", "model-chain.json"),
            Paths.get("..", "..", "parity-fixtures", "phone-control", "model-chain.json"),
            Paths.get("parity-fixtures", "phone-control", "model-chain.json"),
        )
        return candidates.firstOrNull(Files::exists)
            ?: error("Missing Phone Control model-chain fixture. Tried: $candidates")
    }
}
