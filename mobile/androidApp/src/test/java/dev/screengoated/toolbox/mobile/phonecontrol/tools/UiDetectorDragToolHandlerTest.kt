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

class UiDetectorDragToolHandlerTest {
    @Test
    fun registryTreatsSemanticDragAsMutatingBeforeDispatch() {
        val spec = PhoneControlToolRegistry.byName.getValue("drag_target")

        assertEquals(PhoneControlHandler.DRAG_TARGET, spec.handler)
        assertTrue(requireNotNull(spec.handler).mutating)
        assertEquals(listOf("local_ui_detector"), spec.providerIds)
    }

    @Test
    fun selectsBothEndpointsFromOneFrameRefreshesTogetherAndSwipesOnce() = runTest {
        val mapping = dragMapping()
        val backend = DragBackend(mapping)
        val selector = DragSelector()

        val execution = UiDetectorToolHandlers(backend, selector).dragTarget(
            job(),
            dragArgs(),
        )

        assertEquals(1, selector.dragSelections)
        assertTrue(selector.selectionImage.contentEquals(mapping.groundingImageBytes))
        assertEquals(listOf(listOf(FROM_MARK, TO_MARK)), backend.refreshBatches)
        assertEquals(0, backend.singleRefreshCalls)
        assertEquals(listOf("source handle", "destination area"), selector.verificationDescriptions)
        assertEquals(listOf(FROM_MARK, TO_MARK), selector.verificationImages.map { it.single().toInt() })
        assertEquals(1, backend.dragCalls)
        assertEquals(1, backend.clearCalls)
        assertEquals("ok", execution.response.value("code"))
        assertEquals("ready", execution.response.value("provider_state"))
        assertEquals("accessibility", execution.response.value("input_provider"))
        assertEquals("ready", execution.response.value("input_provider_state"))
        assertEquals(FROM_MARK.toString(), execution.response.value("from_mark"))
        assertEquals(TO_MARK.toString(), execution.response.value("to_mark"))
        assertTrue(execution.mutating)
        assertTrue(execution.refreshScreenFrame)
    }

    @Test
    fun rejectedSecondCrosshairPreventsTheSwipeAndRetiresAllMarks() = runTest {
        val backend = DragBackend(dragMapping())
        val selector = DragSelector(
            verifications = ArrayDeque(
                listOf(
                    verificationSuccess(),
                    UiDetectorTargetVerification.Failure(
                        code = "vision_verification_rejected",
                        message = "destination missed",
                        retryable = true,
                        freshObservationRequired = true,
                    ),
                ),
            ),
        )

        val execution = UiDetectorToolHandlers(backend, selector).dragTarget(job(), dragArgs())

        assertEquals(0, backend.dragCalls)
        assertEquals(1, backend.clearCalls)
        assertEquals("vision_verification_rejected", execution.response.value("code"))
        assertEquals("to", execution.response.value("endpoint"))
        assertEquals("proven_no_effect", execution.response.value("effect_status"))
        assertFalse(execution.mutating)
    }

    @Test
    fun generationChangeAfterDualVerificationFailsClosed() = runTest {
        val backend = DragBackend(dragMapping(), observationGeneration = GENERATION + 1)

        val execution = UiDetectorToolHandlers(backend, DragSelector()).dragTarget(job(), dragArgs())

        assertEquals(0, backend.dragCalls)
        assertEquals(1, backend.clearCalls)
        assertEquals("stale_target", execution.response.value("code"))
        assertEquals("true", execution.response.value("fresh_observation_required"))
        assertEquals("proven_no_effect", execution.response.value("effect_status"))
    }

    @Test
    fun mismatchedEndpointVisualRevisionsFailBeforeGestureDispatch() {
        val from = refreshedMark(FROM_MARK, visualRevision = 20)
        val to = refreshedMark(TO_MARK, visualRevision = 21)

        val failure = requireNotNull(detectorDragFrameFailure(from, to))

        assertEquals("stale_frame", failure.code)
        assertEquals(EffectCertainty.PROVEN_NO_EFFECT, failure.effect)
        assertTrue(failure.freshObservationRequired)
    }

    @Test
    fun inputFailureKeepsTheVerifiedDetectorPrimaryReadyAndReportsDependencyState() = runTest {
        val backend = DragBackend(
            mapping = dragMapping(),
            dragResult = AccessibilityProviderResult.Failure(
                code = "provider_failed",
                message = "input dispatch failed",
                retryable = true,
                freshObservationRequired = true,
                effect = EffectCertainty.MAY_HAVE_OCCURRED,
            ),
        )

        val execution = UiDetectorToolHandlers(backend, DragSelector()).dragTarget(job(), dragArgs())

        assertEquals(1, backend.dragCalls)
        assertEquals("local_ui_detector", execution.response.value("provider"))
        assertEquals("ready", execution.response.value("provider_state"))
        assertEquals("accessibility", execution.response.value("input_provider"))
        assertEquals("degraded", execution.response.value("input_provider_state"))
        assertEquals("may_have_occurred", execution.response.value("effect_status"))
        assertTrue(execution.mutating)
    }
}

private class DragBackend(
    private val mapping: UiDetectorMapping,
    override val observationGeneration: Long = GENERATION,
    private val dragResult: AccessibilityProviderResult<AccessibilityGestureOutcome> =
        AccessibilityProviderResult.Success(
            AccessibilityGestureOutcome(
                code = "ok",
                generation = GENERATION + 1,
                effect = EffectCertainty.MAY_HAVE_OCCURRED,
                snapshotInvalidated = true,
            ),
        ),
) : UiDetectorToolBackend {
    val refreshBatches = mutableListOf<List<Int>>()
    var singleRefreshCalls = 0
    var dragCalls = 0
    var clearCalls = 0

    override suspend fun mapCurrentSurface(): UiDetectorProviderResult<UiDetectorMapping> =
        UiDetectorProviderResult.Success(mapping)

    override suspend fun refreshMark(id: Int): UiDetectorProviderResult<UiDetectorRefreshedMark> {
        singleRefreshCalls += 1
        error("drag_target must not refresh endpoints sequentially")
    }

    override suspend fun refreshMarks(
        ids: List<Int>,
    ): UiDetectorProviderResult<UiDetectorRefreshedMarkSet> {
        refreshBatches += ids.toList()
        val marks = ids.map { id -> refreshed(id) }
        return UiDetectorProviderResult.Success(
            UiDetectorRefreshedMarkSet(
                marks = marks,
                inferenceMs = 5,
                observationGeneration = GENERATION,
                surfaceLease = mapping.marks.frame.surfaceLease,
            ),
        )
    }

    override suspend fun activate(
        refreshed: UiDetectorRefreshedMark,
        button: String,
    ): AccessibilityProviderResult<AccessibilityGestureOutcome> =
        error("drag_target must not dispatch a click")

    override suspend fun drag(
        from: UiDetectorRefreshedMark,
        to: UiDetectorRefreshedMark,
        durationMs: Long,
    ): AccessibilityProviderResult<AccessibilityGestureOutcome> {
        assertEquals(FROM_MARK, from.mark.id)
        assertEquals(TO_MARK, to.mark.id)
        assertEquals(from.surfaceLease, to.surfaceLease)
        assertEquals(550L, durationMs)
        dragCalls += 1
        return dragResult
    }

    override fun clearMarks() {
        clearCalls += 1
    }

    private fun refreshed(id: Int): UiDetectorRefreshedMark = UiDetectorRefreshedMark(
        mark = mapping.marks.marks.single { it.id == id },
        overlap = 0.90f,
        inferenceMs = 5,
        observationGeneration = GENERATION,
        surfaceLease = mapping.marks.frame.surfaceLease,
        verificationImageBytes = byteArrayOf(id.toByte()),
    )
}

private class DragSelector(
    private val selection: UiDetectorDragSelection = UiDetectorDragSelection.Success(
        from = UiDetectorTargetSelection.Success(FROM_MARK, 90, "source", "locator"),
        to = UiDetectorTargetSelection.Success(TO_MARK, 92, "destination", "locator"),
    ),
    private val verifications: ArrayDeque<UiDetectorTargetVerification> = ArrayDeque(
        listOf(verificationSuccess(), verificationSuccess()),
    ),
) : UiDetectorTargetSelector {
    var dragSelections = 0
    var selectionImage = byteArrayOf()
    val verificationDescriptions = mutableListOf<String>()
    val verificationImages = mutableListOf<ByteArray>()

    override suspend fun select(
        description: String,
        mapping: UiDetectorMapping,
    ): UiDetectorTargetSelection = error("drag_target must select both endpoints together")

    override suspend fun selectDrag(
        fromDescription: String,
        toDescription: String,
        mapping: UiDetectorMapping,
    ): UiDetectorDragSelection {
        assertEquals("source handle", fromDescription)
        assertEquals("destination area", toDescription)
        dragSelections += 1
        selectionImage = mapping.groundingImageBytes
        return selection
    }

    override suspend fun verify(
        description: String,
        refreshed: UiDetectorRefreshedMark,
    ): UiDetectorTargetVerification {
        verificationDescriptions += description
        verificationImages += refreshed.verificationImageBytes
        return verifications.removeFirst()
    }
}

private fun dragMapping(): UiDetectorMapping {
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
    return UiDetectorMapping(
        marks = UiDetectorMarkSet(
            frame = UiDetectorFrameIdentity(lease, 0, 420, 10),
            marks = listOf(mark(FROM_MARK, 240, 300), mark(TO_MARK, 720, 500)),
        ),
        stats = UiDetectorStats(2, 0, 0, 0),
        inferenceMs = 4,
        executionProvider = "fixture",
        groundingImageBytes = byteArrayOf(1, 2, 3),
    )
}

private fun mark(id: Int, x: Int, y: Int) = UiDetectorMark(
    id,
    UiDetectorBox(x, y, 0.96f, TargetBounds(x - 30, y - 20, x + 30, y + 20)),
)

private fun dragArgs() = buildJsonObject {
    put("from", "source handle")
    put("to", "destination area")
}

private fun verificationSuccess() =
    UiDetectorTargetVerification.Success(95, "endpoint", "locator")

private fun refreshedMark(id: Int, visualRevision: Long): UiDetectorRefreshedMark {
    val mapping = dragMapping()
    return UiDetectorRefreshedMark(
        mark = mapping.marks.marks.single { it.id == id },
        overlap = 0.9f,
        inferenceMs = 4,
        observationGeneration = GENERATION,
        surfaceLease = mapping.marks.frame.surfaceLease,
        verificationImageBytes = byteArrayOf(id.toByte()),
        visualRevision = visualRevision,
    )
}

private fun job() = PhoneControlToolJobContext(1, "drag-job", 2)

private fun kotlinx.serialization.json.JsonObject.value(key: String): String =
    getValue(key).jsonPrimitive.content

private const val GENERATION = 42L
private const val FROM_MARK = 7
private const val TO_MARK = 8
