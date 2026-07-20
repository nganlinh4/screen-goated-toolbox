package dev.screengoated.toolbox.mobile.phonecontrol.provider.detector

import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityObservation
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityTargetAuthority
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityWindowSnapshot
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.surfaceLease
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class UiDetectorModelsTest {
    @Test
    fun postprocessMapsNormalizedBoxesBackToUndistortedScreenCrop() {
        val result = postprocessUiDetector(
            detsShape = longArrayOf(1, 1, 4),
            dets = floatArrayOf(0.05f, 0.5f, 0.2f, 0.4f),
            labelsShape = longArrayOf(1, 1, 1),
            labels = floatArrayOf(4f),
            cropWidth = 200,
            cropHeight = 100,
            originX = -50,
            originY = 20,
        )

        assertEquals(1, result.boxes.size)
        assertEquals(TargetBounds(-50, 50, -20, 90), result.boxes.single().bounds)
        assertEquals(-35, result.boxes.single().centerX)
        assertEquals(70, result.boxes.single().centerY)
    }

    @Test
    fun postprocessRejectsMalformedTensorLengths() {
        val failure = runCatching {
            postprocessUiDetector(
                detsShape = longArrayOf(1, 2, 4),
                dets = FloatArray(4),
                labelsShape = longArrayOf(1, 2, 1),
                labels = FloatArray(2),
                cropWidth = 100,
                cropHeight = 100,
                originX = 0,
                originY = 0,
            )
        }.exceptionOrNull()

        assertTrue(failure?.message.orEmpty().contains("length mismatch"))
    }

    @Test
    fun markSelectionPreservesSpatialCoverageBeforeDenseConfidenceFill() {
        val dense = (0 until 40).map { index ->
            box(50 + index, 50 + index, 0.99f - index / 1_000f)
        }
        val remote = box(1_100, 700, 0.70f)

        val selected = selectUiDetectorMarks(
            dense + remote,
            TargetBounds(0, 0, 1_200, 800),
            limit = 10,
        )

        assertEquals(10, selected.size)
        assertTrue(selected.any { it.centerX == 1_100 && it.centerY == 700 })
    }

    @Test
    fun detectorFrameRequiresTheExactCapturedSurfaceLease() {
        val captured = observation(window())
        val identity = frame(captured)

        assertTrue(identity.matches(captured))
        listOf(
            window(layer = 8),
            window(authority = AccessibilityTargetAuthority.CONSEQUENTIAL),
            window(controllerOwned = true),
        ).forEach { changed ->
            assertFalse(identity.matches(observation(changed)))
        }
    }

    @Test
    fun detectorFrameWireIdentityChangesWithSurfaceAuthority() {
        val routine = frame(observation(window()))
        val consequential = frame(
            observation(window(authority = AccessibilityTargetAuthority.CONSEQUENTIAL)),
        )

        assertNotEquals(routine.wireIdentity, consequential.wireIdentity)
    }

    @Test
    fun freshDetectorMatchingRebindsEveryEndpointToADistinctCandidate() {
        val requested = listOf(
            UiDetectorMark(1, box(100, 100, 0.9f, halfSize = 30)),
            UiDetectorMark(2, box(140, 100, 0.9f, halfSize = 30)),
        )
        val fresh = listOf(
            box(118, 100, 0.95f, halfSize = 30),
            box(151, 100, 0.92f, halfSize = 30),
        )

        val matched = requireNotNull(matchUiDetectorMarks(requested, fresh, 0.20f))

        assertEquals(listOf(1, 2), matched.map { it.requested.id })
        assertEquals(2, matched.map { it.refreshed.bounds }.distinct().size)
    }

    @Test
    fun freshDetectorMatchingFailsWhenTwoEndpointsCollapseToOneCandidate() {
        val requested = listOf(
            UiDetectorMark(1, box(100, 100, 0.9f, halfSize = 30)),
            UiDetectorMark(2, box(110, 100, 0.9f, halfSize = 30)),
        )

        val matched = matchUiDetectorMarks(
            requested,
            candidates = listOf(box(105, 100, 0.95f, halfSize = 30)),
            minimumOverlap = 0.20f,
        )

        assertEquals(null, matched)
    }

    private fun box(x: Int, y: Int, score: Float, halfSize: Int = 5) = UiDetectorBox(
        centerX = x,
        centerY = y,
        score = score,
        bounds = TargetBounds(x - halfSize, y - halfSize, x + halfSize, y + halfSize),
    )

    private fun frame(observation: AccessibilityObservation) = UiDetectorFrameIdentity(
        surfaceLease = requireNotNull(observation.surfaceLease(DISPLAY_ID, WINDOW_ID.toLong())),
        rotation = observation.displayRotation,
        densityDpi = observation.densityDpi,
        capturedAtMs = 900L,
    )

    private fun observation(window: AccessibilityWindowSnapshot) = AccessibilityObservation(
        generation = GENERATION,
        observedAtMs = 800L,
        displayRotation = 0,
        densityDpi = 320,
        windows = listOf(window),
        elements = emptyList(),
        truncated = false,
    )

    private fun window(
        layer: Int = 7,
        authority: AccessibilityTargetAuthority = AccessibilityTargetAuthority.ROUTINE,
        controllerOwned: Boolean = false,
    ) = AccessibilityWindowSnapshot(
        id = WINDOW_ID,
        displayId = DISPLAY_ID,
        layer = layer,
        type = "application",
        title = null,
        packageName = "fixture.surface",
        active = true,
        focused = true,
        bounds = TargetBounds(0, 0, 1_200, 800),
        controllerOwned = controllerOwned,
        targetAuthority = authority,
    )

    private companion object {
        const val GENERATION = 41L
        const val DISPLAY_ID = 0
        const val WINDOW_ID = 12
    }
}
