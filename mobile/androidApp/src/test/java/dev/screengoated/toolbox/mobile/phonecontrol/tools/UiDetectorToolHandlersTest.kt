package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityGestureOutcome
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilitySurfaceLease
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityTargetAuthority
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorBox
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorDragSelection
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorFrameIdentity
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorMapping
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorMark
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorMarkSet
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorRefreshedMark
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorRefreshedMarkSet
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorStats
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorTargetSelection
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorTargetSelector
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorTargetVerification
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class UiDetectorToolHandlersTest {
    @Test
    fun clickTargetSelectsRefreshesAndActivatesInsideTheSameTool() = runTest {
        val mapping = mapping()
        val backend = FakeDetectorBackend(mapping)
        val selector = FakeTargetSelector(
            selection = UiDetectorTargetSelection.Success(7, 93, "visible item", "locator"),
            verification = UiDetectorTargetVerification.Success(96, "visible item", "locator"),
        )
        val handlers = UiDetectorToolHandlers(
            backend = backend,
            targetSelector = selector,
        )

        val execution = handlers.clickTarget(
            job(),
            buildJsonObject {
                put("description", "the requested visible item")
                put("button", "right")
            },
        )

        assertEquals(listOf("the requested visible item"), selector.selectionDescriptions)
        assertEquals(listOf("the requested visible item"), selector.verificationDescriptions)
        assertTrue(selector.selectionImage.contentEquals(mapping.groundingImageBytes))
        assertTrue(selector.verificationImage.contentEquals(byteArrayOf(4, 5, 6)))
        assertEquals(listOf(7), backend.refreshedMarks)
        assertEquals(listOf("right"), backend.activatedButtons)
        assertTrue(backend.cleared)
        assertEquals("click_target", execution.response.value("requested_tool"))
        assertEquals("ok", execution.response.value("code"))
        assertEquals("7", execution.response.value("clicked_mark"))
        assertEquals("locator", execution.response.value("target_selection_model"))
        assertEquals("93", execution.response.value("target_selection_confidence"))
        assertEquals("96", execution.response.value("target_verification_confidence"))
        assertTrue(execution.mutating)
        assertTrue(execution.refreshScreenFrame)
    }

    @Test
    fun selectionFailureCannotReachInputDispatch() = runTest {
        val backend = FakeDetectorBackend(mapping())
        val handlers = UiDetectorToolHandlers(
            backend = backend,
            targetSelector = FakeTargetSelector(
                selection = UiDetectorTargetSelection.Failure(
                    code = "target_not_found",
                    message = "not visible",
                    retryable = false,
                ),
            ),
        )

        val execution = handlers.clickTarget(
            job(),
            buildJsonObject { put("description", "missing item") },
        )

        assertTrue(backend.refreshedMarks.isEmpty())
        assertTrue(backend.activatedButtons.isEmpty())
        assertTrue(backend.cleared)
        assertEquals("target_not_found", execution.response.value("code"))
        assertEquals("proven_no_effect", execution.response.value("effect_status"))
        assertFalse(execution.mutating)
    }

    @Test
    fun generationChangeAfterFreshDetectorVerificationBlocksGesture() = runTest {
        val backend = FakeDetectorBackend(mapping(), observationGeneration = GENERATION + 1)
        val handlers = UiDetectorToolHandlers(
            backend = backend,
            targetSelector = FakeTargetSelector(
                selection = UiDetectorTargetSelection.Success(7, 80, null, "locator"),
                verification = UiDetectorTargetVerification.Success(80, null, "locator"),
            ),
        )

        val execution = handlers.clickTarget(
            job(),
            buildJsonObject { put("description", "visible item") },
        )

        assertEquals(listOf(7), backend.refreshedMarks)
        assertTrue(backend.activatedButtons.isEmpty())
        assertTrue(backend.cleared)
        assertEquals("stale_target", execution.response.value("code"))
        assertEquals("true", execution.response.value("fresh_observation_required"))
        assertFalse(execution.mutating)
    }

    @Test
    fun semanticCrosshairRejectionBlocksGestureWithAProvenNoEffectReceipt() = runTest {
        val backend = FakeDetectorBackend(mapping())
        val handlers = UiDetectorToolHandlers(
            backend = backend,
            targetSelector = FakeTargetSelector(
                selection = UiDetectorTargetSelection.Success(7, 88, null, "locator"),
                verification = UiDetectorTargetVerification.Failure(
                    code = "vision_verification_rejected",
                    message = "crosshair missed",
                    retryable = true,
                    freshObservationRequired = true,
                ),
            ),
        )

        val execution = handlers.clickTarget(
            job(),
            buildJsonObject { put("description", "visible item") },
        )

        assertEquals(listOf(7), backend.refreshedMarks)
        assertTrue(backend.activatedButtons.isEmpty())
        assertTrue(backend.cleared)
        assertEquals("vision_verification_rejected", execution.response.value("code"))
        assertEquals("proven_no_effect", execution.response.value("effect_status"))
        assertEquals("true", execution.response.value("fresh_observation_required"))
        assertFalse(execution.mutating)
    }

    @Test
    fun refreshFailureAlwaysRetiresTheCapturedMarkSet() = runTest {
        val backend = FakeDetectorBackend(
            mapping = mapping(),
            refreshFailure = UiDetectorProviderResult.Failure(
                code = "detector_inference_failed",
                message = "refresh failed",
                retryable = true,
                freshObservationRequired = false,
            ),
        )
        val handlers = UiDetectorToolHandlers(
            backend = backend,
            targetSelector = FakeTargetSelector(
                selection = UiDetectorTargetSelection.Success(7, 88, null, "locator"),
            ),
        )

        val execution = handlers.clickTarget(
            job(),
            buildJsonObject { put("description", "visible item") },
        )

        assertEquals(listOf(7), backend.refreshedMarks)
        assertTrue(backend.activatedButtons.isEmpty())
        assertTrue(backend.cleared)
        assertEquals("detector_inference_failed", execution.response.value("code"))
        assertEquals("proven_no_effect", execution.response.value("effect_status"))
        assertFalse(execution.mutating)
    }
}

private class FakeDetectorBackend(
    private val mapping: UiDetectorMapping,
    override val observationGeneration: Long = GENERATION,
    private val refreshFailure: UiDetectorProviderResult.Failure? = null,
) : UiDetectorToolBackend {
    val refreshedMarks = mutableListOf<Int>()
    val activatedButtons = mutableListOf<String>()
    var cleared = false

    override suspend fun mapCurrentSurface(): UiDetectorProviderResult<UiDetectorMapping> =
        UiDetectorProviderResult.Success(mapping)

    override suspend fun refreshMark(id: Int): UiDetectorProviderResult<UiDetectorRefreshedMark> {
        return when (val result = refreshMarks(listOf(id))) {
            is UiDetectorProviderResult.Failure -> result
            is UiDetectorProviderResult.Success ->
                UiDetectorProviderResult.Success(result.value.marks.single())
        }
    }

    override suspend fun refreshMarks(
        ids: List<Int>,
    ): UiDetectorProviderResult<UiDetectorRefreshedMarkSet> {
        refreshedMarks += ids
        refreshFailure?.let { return it }
        val marks = ids.distinct().map { id ->
            UiDetectorRefreshedMark(
                mark = mapping.marks.marks.single { it.id == id },
                overlap = 0.91f,
                inferenceMs = 4,
                observationGeneration = GENERATION,
                surfaceLease = mapping.marks.frame.surfaceLease,
                verificationImageBytes = byteArrayOf(4, 5, 6),
            )
        }
        return UiDetectorProviderResult.Success(
            UiDetectorRefreshedMarkSet(
                marks = marks,
                inferenceMs = 4,
                observationGeneration = GENERATION,
                surfaceLease = mapping.marks.frame.surfaceLease,
            ),
        )
    }

    override suspend fun activate(
        refreshed: UiDetectorRefreshedMark,
        button: String,
    ): AccessibilityProviderResult<AccessibilityGestureOutcome> {
        activatedButtons += button
        return AccessibilityProviderResult.Success(
            AccessibilityGestureOutcome(
                code = "ok",
                generation = GENERATION + 1,
                effect = EffectCertainty.MAY_HAVE_OCCURRED,
                snapshotInvalidated = true,
            ),
        )
    }

    override suspend fun drag(
        from: UiDetectorRefreshedMark,
        to: UiDetectorRefreshedMark,
        durationMs: Long,
    ): AccessibilityProviderResult<AccessibilityGestureOutcome> =
        error("drag should not run in click handler tests")

    override fun clearMarks() {
        cleared = true
    }
}

private class FakeTargetSelector(
    private val selection: UiDetectorTargetSelection,
    private val verification: UiDetectorTargetVerification = UiDetectorTargetVerification.Failure(
        code = "unexpected_verification",
        message = "verification should not run",
        retryable = false,
        freshObservationRequired = false,
    ),
) : UiDetectorTargetSelector {
    val selectionDescriptions = mutableListOf<String>()
    val verificationDescriptions = mutableListOf<String>()
    var selectionImage = byteArrayOf()
    var verificationImage = byteArrayOf()

    override suspend fun select(
        description: String,
        mapping: UiDetectorMapping,
    ): UiDetectorTargetSelection {
        selectionDescriptions += description
        selectionImage = mapping.groundingImageBytes
        return selection
    }

    override suspend fun verify(
        description: String,
        refreshed: UiDetectorRefreshedMark,
    ): UiDetectorTargetVerification {
        verificationDescriptions += description
        verificationImage = refreshed.verificationImageBytes
        return verification
    }

    override suspend fun selectDrag(
        fromDescription: String,
        toDescription: String,
        mapping: UiDetectorMapping,
    ): UiDetectorDragSelection = UiDetectorDragSelection.Failure(
        code = "unexpected_drag_selection",
        message = "drag selection should not run in click handler tests",
        retryable = false,
    )
}

private fun mapping(): UiDetectorMapping {
    val lease = AccessibilitySurfaceLease(
        observationGeneration = GENERATION,
        displayId = 0,
        windowId = 12,
        packageOrSurface = "fixture.surface",
        windowLayer = 4,
        bounds = TargetBounds(0, 0, 1_000, 800),
        authority = AccessibilityTargetAuthority.ROUTINE,
        controllerOwned = false,
    )
    val mark = UiDetectorMark(
        id = 7,
        box = UiDetectorBox(
            centerX = 320,
            centerY = 240,
            score = 0.97f,
            bounds = TargetBounds(280, 210, 360, 270),
        ),
    )
    return UiDetectorMapping(
        marks = UiDetectorMarkSet(
            frame = UiDetectorFrameIdentity(lease, rotation = 0, densityDpi = 420, capturedAtMs = 10),
            marks = listOf(mark),
        ),
        stats = UiDetectorStats(1, 0, 0, 0),
        inferenceMs = 3,
        executionProvider = "fixture",
        groundingImageBytes = byteArrayOf(1, 2, 3),
    )
}

private fun job() = PhoneControlToolJobContext(1, "detector-job", 2)

private fun kotlinx.serialization.json.JsonObject.value(key: String): String =
    getValue(key).jsonPrimitive.content

private const val GENERATION = 42L
