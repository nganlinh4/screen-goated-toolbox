package dev.screengoated.toolbox.mobile.phonecontrol.provider.detector

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class UiDetectorTargetSelectorTest {
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
}
