package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityActionOutcome
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityActionVerb
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityElement
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityGestureOutcome
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityObservation
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityMutationKind
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilitySurfaceLease
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityWindowSnapshot
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.surfaceLease as accessibilitySurfaceLease
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.result.PhoneControlTargetIdentity
import dev.screengoated.toolbox.mobile.phonecontrol.result.TargetBounds
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AccessibilityToolHandlersTest {
    @Test
    fun providerOsOwnedRefusalStaysNoEffectForBothConfirmValues() = runTest {
        val backend = FakeAccessibilityBackend(AccessibilityObservationFrame(observation(15)))
        repeat(2) {
            backend.actionOutcomes += AccessibilityProviderResult.Failure(
                code = "os_owned_confirmation",
                message = "Android owns this step.",
                retryable = true,
                requiredUserStep = "complete_os_owned_confirmation",
            )
        }

        listOf(false, true).forEach { confirmed ->
            val execution = handleAct(
                JOB,
                jsonArgs("id" to 7, "verb" to "click", "confirm" to confirmed),
                backend = backend,
            )

            assertEquals("os_owned_confirmation", execution.response.stringValue("code"))
            assertEquals("proven_no_effect", execution.response.stringValue("effect_status"))
            assertEquals(false, execution.response.getValue("executed").jsonPrimitive.content.toBoolean())
            assertEquals(
                "complete_os_owned_confirmation",
                execution.response.getValue("required_user_step").jsonObject.stringValue("code"),
            )
            assertFalse(execution.mutating)
            assertFalse(execution.refreshScreenFrame)
        }
        assertEquals(listOf(false, true), backend.actions.map { it.confirmed })
    }

    @Test
    fun consequentialTargetRequiresBooleanConfirmationBeforeDispatch() = runTest {
        val backend = FakeAccessibilityBackend(AccessibilityObservationFrame(observation(16)))
        backend.targets[8] = target(8, 16)
        backend.actionOutcomes += AccessibilityProviderResult.Failure(
            code = "confirmation_required",
            message = "Confirmation is required.",
            retryable = true,
            requiredUserStep = "confirm_consequential_action",
        )
        backend.actionOutcomes += AccessibilityProviderResult.Success(
            AccessibilityActionOutcome(
                code = "ok",
                generation = 17,
                effect = EffectCertainty.MAY_HAVE_OCCURRED,
                snapshotInvalidated = true,
                freshObservationRequired = true,
            ),
        )

        val blocked = handleAct(JOB, jsonArgs("id" to 8, "verb" to "click"), backend = backend)
        val malformed = handleAct(
            JOB,
            jsonArgs("id" to 8, "verb" to "click", "confirm" to "yes"),
            backend = backend,
        )
        val allowed = handleAct(
            JOB,
            jsonArgs("id" to 8, "verb" to "click", "confirm" to true),
            backend = backend,
        )

        assertEquals("confirmation_required", blocked.response.stringValue("code"))
        assertFalse(blocked.mutating)
        assertEquals("invalid_arguments", malformed.response.stringValue("code"))
        assertFalse(malformed.mutating)
        assertEquals("ok", allowed.response.stringValue("code"))
        assertTrue(allowed.mutating)
        assertEquals(listOf(8, 8), backend.actions.map { it.targetId })
        assertEquals(listOf(false, true), backend.actions.map { it.confirmed })
    }

    @Test
    fun provenNoEffectActionReceiptIsNotReportedAsMutating() = runTest {
        val backend = FakeAccessibilityBackend(AccessibilityObservationFrame(observation(18)))
        backend.targets[9] = target(9, 18)
        backend.actionOutcomes += AccessibilityProviderResult.Success(
            AccessibilityActionOutcome(
                code = "action_rejected",
                generation = 18,
                effect = EffectCertainty.PROVEN_NO_EFFECT,
                snapshotInvalidated = false,
                freshObservationRequired = false,
            ),
        )

        val execution = handleAct(JOB, jsonArgs("id" to 9, "verb" to "click"), backend = backend)

        assertEquals("action_rejected", execution.response.stringValue("code"))
        assertFalse(execution.mutating)
        assertFalse(execution.refreshScreenFrame)
    }

    @Test
    fun gridActionsRequireIdentityMatchingTheCurrentObservation() = runTest {
        val observation = observation(generation = 5)
        val backend = FakeAccessibilityBackend(AccessibilityObservationFrame(observation))
        val noGrid = handleClickAt(JOB, jsonArgs("cell" to 5), backend)
        assertTypedStaleGrid(noGrid)
        assertEquals(0, backend.clicks.size)

        val capturedGrid = matchingGrid(observation)
        backend.frame = AccessibilityObservationFrame(
            observation = observation,
            grid = capturedGrid.copy(
                observationGeneration = 4,
                surfaceLease = capturedGrid.surfaceLease.copy(observationGeneration = 4),
            ),
        )
        val staleGrid = handleClickAt(JOB, jsonArgs("cell" to 5), backend)
        assertTypedStaleGrid(staleGrid)
        assertEquals(0, backend.clicks.size)

        backend.frame = AccessibilityObservationFrame(
            observation = observation,
            grid = matchingGrid(observation),
        )
        backend.gestureOutcomes += AccessibilityProviderResult.Success(
            AccessibilityGestureOutcome(
                code = "ok",
                generation = observation.generation,
                effect = EffectCertainty.MAY_HAVE_OCCURRED,
                snapshotInvalidated = true,
            ),
        )
        val dispatched = handleClickAt(JOB, jsonArgs("cell" to 5), backend)

        assertEquals("ok", dispatched.response.stringValue("code"))
        assertEquals(1, backend.clicks.size)
        assertEquals(150f, backend.clicks.single().x, 0.001f)
        assertEquals(300f, backend.clicks.single().y, 0.001f)
        assertEquals("dev.test", backend.clicks.single().lease.packageOrSurface)
        assertEquals(0, backend.clicks.single().lease.windowLayer)
        assertEquals(77L, backend.clicks.single().expectedVisualRevision)
    }

    @Test
    fun dragAndCellScrollDoNotSynthesizeAGridFromWindowBounds() = runTest {
        val backend = FakeAccessibilityBackend(
            AccessibilityObservationFrame(observation(generation = 9)),
        )

        val drag = handleDrag(
            JOB,
            jsonArgs("from_cell" to 1, "to_cell" to 2),
            backend,
        )
        val scroll = handleScroll(
            JOB,
            jsonArgs("direction" to "down", "cell" to 4),
            backend,
        )

        assertTypedStaleGrid(drag)
        assertTypedStaleGrid(scroll)
        assertEquals(0, backend.swipes.size)
    }

    @Test
    fun gridlessScrollStaysUsefulWhileCellScrollCarriesItsVisualRevision() = runTest {
        val observation = observation(generation = 10)
        val backend = FakeAccessibilityBackend(AccessibilityObservationFrame(observation))
        repeat(2) {
            backend.gestureOutcomes += AccessibilityProviderResult.Success(
                AccessibilityGestureOutcome(
                    code = "ok",
                    generation = observation.generation,
                    effect = EffectCertainty.MAY_HAVE_OCCURRED,
                    snapshotInvalidated = true,
                ),
            )
        }

        val gridless = handleScroll(JOB, jsonArgs("direction" to "down"), backend)
        backend.frame = AccessibilityObservationFrame(
            observation = observation,
            grid = matchingGrid(observation),
        )
        val cellBound = handleScroll(
            JOB,
            jsonArgs("direction" to "up", "cell" to 4),
            backend,
        )

        assertEquals("ok", gridless.response.stringValue("code"))
        assertEquals("ok", cellBound.response.stringValue("code"))
        assertEquals(2, backend.swipes.size)
        assertEquals(null, backend.swipes[0].expectedVisualRevision)
        assertEquals(77L, backend.swipes[1].expectedVisualRevision)
    }

    @Test
    fun batchReportsTotalRequestedAttemptedCompletedAndPerStepCapabilities() = runTest {
        val first = target(id = 1, generation = 3)
        val second = target(id = 2, generation = 3)
        val third = target(id = 3, generation = 3)
        val freshSecond = second.copy(snapshotGeneration = 4, observationTimestampMs = 4)
        val freshThird = third.copy(snapshotGeneration = 4, observationTimestampMs = 4)
        val backend = FakeAccessibilityBackend(
            AccessibilityObservationFrame(
                observation(
                    generation = 4,
                    elements = listOf(
                        element(id = 22, target = freshSecond),
                        element(id = 33, target = freshThird),
                    ),
                ),
            ),
        )
        backend.targets += mapOf(1 to first, 2 to second, 3 to third)
        backend.actionOutcomes += AccessibilityProviderResult.Success(
            AccessibilityActionOutcome(
                code = "ok",
                generation = 4,
                effect = EffectCertainty.VERIFIED,
                snapshotInvalidated = true,
                freshObservationRequired = true,
            ),
        )
        backend.actionOutcomes += AccessibilityProviderResult.Success(
            AccessibilityActionOutcome(
                code = "action_rejected",
                generation = 5,
                effect = EffectCertainty.PROVEN_NO_EFFECT,
                snapshotInvalidated = false,
                freshObservationRequired = false,
            ),
        )
        val steps = buildJsonObject {
            put(
                "steps",
                buildJsonArray {
                    add(jsonArgs("id" to 1, "verb" to "click"))
                    add(jsonArgs("id" to 2, "verb" to "fill", "value" to "hello"))
                    add(jsonArgs("id" to 3, "verb" to "toggle"))
                },
            )
        }

        val execution = handleDoSteps(JOB, steps, backend)
        val response = execution.response
        val results = response.getValue("results").jsonArray

        assertEquals("partial", response.stringValue("code"))
        assertEquals("ui.pointer_action", response.stringValue("capability"))
        assertEquals(3, response.intValue("requested"))
        assertEquals(2, response.intValue("attempted"))
        assertEquals(1, response.intValue("completed"))
        assertTrue(response.getValue("stopped").jsonPrimitive.content.toBoolean())
        assertEquals(
            setOf("ui.pointer_action", "ui.text_edit"),
            response.getValue("capabilities").jsonArray.map { it.jsonPrimitive.content }.toSet(),
        )
        assertEquals(2, results.size)
        assertEquals("ui.pointer_action", results[0].jsonObject.stringValue("capability"))
        assertEquals("ui.text_edit", results[1].jsonObject.stringValue("capability"))
        assertEquals(listOf(1, 22), backend.actions.map { it.targetId })
        assertEquals(listOf(AccessibilityActionVerb.CLICK, AccessibilityActionVerb.FILL), backend.actions.map { it.verb })
        assertTrue(execution.mutating)
    }

    @Test
    fun staleBatchBeforeDispatchProvesNoEffectAndKeepsRequestedCount() = runTest {
        val backend = FakeAccessibilityBackend(
            AccessibilityObservationFrame(observation(generation = 12)),
        )
        val args = buildJsonObject {
            put("steps", buildJsonArray { add(jsonArgs("id" to 77, "verb" to "click")) })
        }

        val execution = handleDoSteps(JOB, args, backend)

        assertEquals("stale_target", execution.response.stringValue("code"))
        assertEquals("proven_no_effect", execution.response.stringValue("effect_status"))
        assertEquals(1, execution.response.intValue("requested"))
        assertEquals(0, execution.response.intValue("attempted"))
        assertEquals(0, execution.response.intValue("completed"))
        assertFalse(execution.mutating)
        assertTrue(backend.actions.isEmpty())
    }

    @Test
    fun staleRecoveryAttachesCurrentActionableObservationWithoutDispatch() = runTest {
        val currentTarget = target(id = 5, generation = 41)
        val backend = FakeAccessibilityBackend(
            AccessibilityObservationFrame(
                observation(
                    generation = 41,
                    elements = listOf(element(id = 5, target = currentTarget)),
                ),
            ),
        )
        val stale = PhoneControlToolExecution(
            response = toolResponse(
                job = JOB,
                requestedTool = "act",
                capability = "ui.pointer_action",
                provider = "accessibility",
                providerState = CapabilityState.READY,
                code = "stale_target",
                observationGeneration = 40,
                effect = EffectCertainty.PROVEN_NO_EFFECT,
                snapshotInvalidated = false,
                retryable = true,
                freshObservationRequired = true,
            ),
            mutating = false,
        )

        val recovered = AndroidActionableObservationRecovery(backend).recover(stale)

        assertEquals("stale_target", recovered.response.stringValue("code"))
        assertEquals(40L, recovered.response.getValue("attempted_observation_generation").jsonPrimitive.long)
        assertEquals(41L, recovered.response.getValue("observation_generation").jsonPrimitive.long)
        assertTrue(recovered.response.getValue("state_reconciled").jsonPrimitive.boolean)
        assertTrue(recovered.response.getValue("fresh_observation_attached").jsonPrimitive.boolean)
        assertFalse(recovered.response.getValue("fresh_observation_required").jsonPrimitive.boolean)
        assertTrue(recovered.response.stringValue("elements").contains("@5"))
        assertEquals(
            "@android-window:v1:41:0:1:dev.test",
            recovered.response.getValue("surface_targets").jsonArray.single()
                .jsonObject.stringValue("target"),
        )
        assertTrue(recovered.refreshScreenFrame)
        assertFalse(recovered.mutating)
        assertTrue(backend.actions.isEmpty())
    }

    private fun assertTypedStaleGrid(execution: PhoneControlToolExecution) {
        assertEquals("stale_frame", execution.response.stringValue("code"))
        assertEquals("ui.pointer_action", execution.response.stringValue("capability"))
        assertEquals("accessibility", execution.response.stringValue("provider"))
        assertEquals("proven_no_effect", execution.response.stringValue("effect_status"))
        assertTrue(execution.response.getValue("retryable").jsonPrimitive.boolean)
        assertTrue(execution.response.getValue("fresh_observation_required").jsonPrimitive.boolean)
        assertFalse(execution.mutating)
        assertTrue(execution.refreshScreenFrame)
    }

    private class FakeAccessibilityBackend(
        var frame: AccessibilityObservationFrame,
    ) : AccessibilityToolBackend {
        data class ActionCall(
            val targetId: Int,
            val verb: AccessibilityActionVerb,
            val value: String?,
            val confirmed: Boolean,
        )

        data class ClickCall(
            val lease: AccessibilitySurfaceLease,
            val x: Float,
            val y: Float,
            val expectedVisualRevision: Long?,
        )

        data class SwipeCall(
            val lease: AccessibilitySurfaceLease,
            val fromX: Float,
            val fromY: Float,
            val toX: Float,
            val toY: Float,
            val durationMs: Long,
            val kind: AccessibilityMutationKind,
            val expectedVisualRevision: Long?,
        )

        override val isReady: Boolean = true
        override val observationGeneration: Long
            get() = frame.observation.generation
        val targets = mutableMapOf<Int, PhoneControlTargetIdentity>()
        val actionOutcomes = mutableListOf<AccessibilityProviderResult<AccessibilityActionOutcome>>()
        val gestureOutcomes = mutableListOf<AccessibilityProviderResult<AccessibilityGestureOutcome>>()
        val actions = mutableListOf<ActionCall>()
        val clicks = mutableListOf<ClickCall>()
        val swipes = mutableListOf<SwipeCall>()

        override suspend fun observe(): AccessibilityProviderResult<AccessibilityObservationFrame> =
            AccessibilityProviderResult.Success(frame)

        override fun currentTargetIdentity(targetId: Int): PhoneControlTargetIdentity? =
            targets[targetId]

        override suspend fun act(
            targetId: Int,
            verb: AccessibilityActionVerb,
            value: String?,
            confirmed: Boolean,
        ): AccessibilityProviderResult<AccessibilityActionOutcome> {
            actions += ActionCall(targetId, verb, value, confirmed)
            return actionOutcomes.removeFirstOrFail("action")
        }

        override suspend fun click(
            lease: AccessibilitySurfaceLease,
            x: Float,
            y: Float,
            expectedVisualRevision: Long?,
        ): AccessibilityProviderResult<AccessibilityGestureOutcome> {
            clicks += ClickCall(lease, x, y, expectedVisualRevision)
            return gestureOutcomes.removeFirstOrFail("click")
        }

        override suspend fun swipe(
            lease: AccessibilitySurfaceLease,
            fromX: Float,
            fromY: Float,
            toX: Float,
            toY: Float,
            durationMs: Long,
            kind: AccessibilityMutationKind,
            expectedVisualRevision: Long?,
        ): AccessibilityProviderResult<AccessibilityGestureOutcome> {
            swipes += SwipeCall(
                lease,
                fromX,
                fromY,
                toX,
                toY,
                durationMs,
                kind,
                expectedVisualRevision,
            )
            return gestureOutcomes.removeFirstOrFail("swipe")
        }
    }

    private companion object {
        val WINDOW_BOUNDS = TargetBounds(0, 0, 300, 600)
        val JOB = PhoneControlToolJobContext(
            turnId = 4,
            jobId = "job-accessibility-test",
            responseGeneration = 8,
        )

        fun matchingGrid(observation: AccessibilityObservation) = AccessibilityGridIdentity(
            observationGeneration = observation.generation,
            visualRevision = 77,
            displayId = 0,
            windowId = 1,
            bounds = WINDOW_BOUNDS,
            columns = 3,
            rows = 3,
            surfaceLease = requireNotNull(observation.accessibilitySurfaceLease(0, 1)),
            rotation = observation.displayRotation,
            densityDpi = observation.densityDpi,
            capturedAtMs = observation.observedAtMs,
        )

        fun observation(
            generation: Long,
            elements: List<AccessibilityElement> = emptyList(),
        ) = AccessibilityObservation(
            generation = generation,
            observedAtMs = generation,
            displayRotation = 0,
            densityDpi = 320,
            windows = listOf(
                AccessibilityWindowSnapshot(
                    id = 1,
                    displayId = 0,
                    layer = 0,
                    type = "application",
                    title = "Test",
                    packageName = "dev.test",
                    active = true,
                    focused = true,
                    bounds = WINDOW_BOUNDS,
                ),
            ),
            elements = elements,
            truncated = false,
        )

        fun target(id: Int, generation: Long) = PhoneControlTargetIdentity(
            snapshotGeneration = generation,
            displayId = 0,
            windowId = 1,
            packageOrSurface = "dev.test",
            nodeOrDocumentIdentity = "node-$id",
            bounds = TargetBounds(id, id, id + 20, id + 20),
            observationTimestampMs = generation,
        )

        fun element(id: Int, target: PhoneControlTargetIdentity) = AccessibilityElement(
            id = id,
            role = "button",
            label = "Target $id",
            value = null,
            hint = null,
            stateDescription = null,
            viewId = "dev.test:id/target_$id",
            packageName = target.packageOrSurface,
            className = "android.widget.Button",
            bounds = target.bounds,
            actions = setOf("click", "set_text"),
            enabled = true,
            visible = true,
            focused = false,
            selected = false,
            checked = null,
            controllerOwned = false,
            target = target,
        )

        fun jsonArgs(vararg values: Pair<String, Any>): JsonObject = buildJsonObject {
            values.forEach { (name, value) ->
                when (value) {
                    is Int -> put(name, value)
                    is String -> put(name, value)
                    is Boolean -> put(name, value)
                    else -> error("Unsupported test JSON value: ${value::class.java.name}")
                }
            }
        }

        fun JsonObject.stringValue(name: String): String =
            getValue(name).jsonPrimitive.contentOrNull ?: error("Missing $name")

        fun JsonObject.intValue(name: String): Int = getValue(name).jsonPrimitive.int

        fun <T> MutableList<T>.removeFirstOrFail(kind: String): T {
            check(isNotEmpty()) { "No queued $kind result" }
            return removeAt(0)
        }
    }
}
