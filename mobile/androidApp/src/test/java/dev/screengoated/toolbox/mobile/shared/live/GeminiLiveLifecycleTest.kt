package dev.screengoated.toolbox.mobile.shared.live

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.add
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class GeminiLiveLifecycleTest {
    @Test
    fun `shared signed inputs reject negative values`() {
        assertThrows(IllegalArgumentException::class.java) {
            GeminiLiveLifecyclePolicy.continuous().copy(setupTimeoutMs = -1)
        }
        assertThrows(IllegalArgumentException::class.java) {
            GeminiLiveBackoffPolicy(jitterSpan = -1)
        }
        assertThrows(IllegalArgumentException::class.java) {
            GeminiLiveBackoffPolicy().delayMs(-1)
        }
        assertThrows(IllegalArgumentException::class.java) {
            GeminiLiveLifecycleEvent.InputSent(chunks = -1)
        }
        assertThrows(IllegalArgumentException::class.java) {
            GeminiLiveLifecycleEvent.WorkState(
                pendingWorkCount = -1,
                bufferedInputCount = 0,
                userSpeaking = false,
            )
        }
        assertThrows(IllegalArgumentException::class.java) {
            GeminiLiveLifecycleFrame(generation = 1, contentCount = -1)
        }
        assertThrows(IllegalArgumentException::class.java) {
            GeminiLiveLifecycleFrame(generation = 1, goAwayTimeLeftMs = -1)
        }
        assertThrows(IllegalArgumentException::class.java) {
            GeminiLiveSessionLifecycle(GeminiLiveLifecyclePolicy.continuous())
                .reduce(-1, GeminiLiveLifecycleEvent.Start)
        }
    }

    @Test
    fun `backoff is total for extreme public policy values`() {
        val policy = GeminiLiveBackoffPolicy(
            baseMs = Long.MAX_VALUE,
            exponentCap = Int.MAX_VALUE,
            jitterMinMs = Long.MAX_VALUE,
            jitterSeed = Long.MAX_VALUE,
            jitterStep = Long.MAX_VALUE,
            jitterSpan = Long.MAX_VALUE,
            maxMs = 6_000,
        )

        assertEquals(6_000L, policy.delayMs(Int.MAX_VALUE))
    }

    @Test
    fun `backoff jitter saturates at the shared signed long boundary`() {
        val policy = GeminiLiveBackoffPolicy(
            baseMs = 0,
            exponentCap = 0,
            jitterMinMs = 0,
            jitterSeed = Long.MAX_VALUE,
            jitterStep = 1,
            jitterSpan = 180,
            maxMs = Long.MAX_VALUE,
        )

        assertEquals(Long.MAX_VALUE % 180, policy.delayMs(1))
    }

    @Test
    fun `lifecycle matches shared parity fixture`() {
        val fixture = Json.parseToJsonElement(
            File(repoRoot(), FIXTURE_PATH).readText(),
        ).jsonObject
        val backoff = backoffPolicy(fixture.getValue("backoffFormula").jsonObject)
        val profiles = fixture.getValue("profiles").jsonObject
        val arrangements = fixture.getValue("arrangements").jsonObject

        fixture.getValue("cases").jsonArray.forEach { caseElement ->
            val case = caseElement.jsonObject
            val name = case.getValue("name").jsonPrimitive.content
            val profileName = case.getValue("profile").jsonPrimitive.content
            val lifecycle = GeminiLiveSessionLifecycle(
                policy = lifecyclePolicy(profiles.getValue(profileName).jsonObject),
                backoff = backoff,
            )
            case["arrange"]?.jsonPrimitive?.contentOrNull?.let { arrangement ->
                replaySteps(
                    lifecycle = lifecycle,
                    steps = arrangements.getValue(arrangement).jsonArray,
                    caseName = name,
                    assertExpected = false,
                )
            }
            replaySteps(
                lifecycle = lifecycle,
                steps = case.getValue("steps").jsonArray,
                caseName = name,
                assertExpected = true,
            )
        }
    }

    private fun replaySteps(
        lifecycle: GeminiLiveSessionLifecycle,
        steps: JsonArray,
        caseName: String,
        assertExpected: Boolean,
    ) {
        steps.forEach { stepElement ->
            val step = stepElement.jsonObject
            val atMs = step.getValue("atMs").jsonPrimitive.long
            val effects = lifecycle.reduce(atMs, event(step.getValue("event").jsonObject))
            if (!assertExpected) return@forEach

            assertState(
                actual = lifecycle.state,
                expected = step.getValue("expectState").jsonObject,
                caseName = caseName,
            )
            assertEquals(
                "effects mismatch in $caseName at ${atMs}ms",
                step.getValue("expectEffects"),
                effectsJson(effects),
            )
        }
    }

    private fun event(value: JsonObject): GeminiLiveLifecycleEvent {
        return when (val type = value.getValue("type").jsonPrimitive.content) {
            "start" -> GeminiLiveLifecycleEvent.Start
            "socketOpened" -> GeminiLiveLifecycleEvent.SocketOpened(
                generation = value.getValue("generation").jsonPrimitive.long,
            )
            "frame" -> GeminiLiveLifecycleEvent.Frame(
                GeminiLiveLifecycleFrame(
                    generation = value.getValue("generation").jsonPrimitive.long,
                    contentCount = optionalLong(value, "contentCount")?.toInt() ?: 0,
                    setupComplete = booleanField(value, "setupComplete"),
                    turnComplete = booleanField(value, "turnComplete"),
                    generationComplete = booleanField(value, "generationComplete"),
                    interrupted = booleanField(value, "interrupted"),
                    goAwayTimeLeftMs = optionalLong(value, "goAwayTimeLeftMs"),
                    toolCallIds = stringArray(value["toolCallIds"]),
                    toolCancellationIds = stringArray(value["toolCancellationIds"]),
                    error = value["error"]
                        ?.takeUnless { it is JsonNull }
                        ?.jsonObject
                        ?.let { error ->
                            GeminiLiveClassifiedError(
                                kind = error.getValue("kind").jsonPrimitive.content,
                                retryable = error.getValue("retryable").jsonPrimitive.boolean,
                            )
                        },
                ),
            )
            "transportFailure" -> GeminiLiveLifecycleEvent.TransportFailure(
                generation = value.getValue("generation").jsonPrimitive.long,
                retryable = value.getValue("retryable").jsonPrimitive.boolean,
            )
            "inputSent" -> GeminiLiveLifecycleEvent.InputSent(
                chunks = value.getValue("chunks").jsonPrimitive.long,
            )
            "inputActivity" -> GeminiLiveLifecycleEvent.InputActivity
            "workState" -> GeminiLiveLifecycleEvent.WorkState(
                pendingWorkCount = value.getValue("pendingWorkCount").jsonPrimitive.long,
                bufferedInputCount = value.getValue("bufferedInputCount").jsonPrimitive.long,
                userSpeaking = value.getValue("userSpeaking").jsonPrimitive.boolean,
            )
            "tick" -> GeminiLiveLifecycleEvent.Tick
            "cancel" -> GeminiLiveLifecycleEvent.Cancel
            else -> error("unknown lifecycle event $type")
        }
    }

    private fun lifecyclePolicy(value: JsonObject): GeminiLiveLifecyclePolicy {
        val kind = when (val name = value.getValue("kind").jsonPrimitive.content) {
            "finiteRequest" -> GeminiLiveSessionKind.FINITE_REQUEST
            "continuousStream" -> GeminiLiveSessionKind.CONTINUOUS_STREAM
            "agentSession" -> GeminiLiveSessionKind.AGENT_SESSION
            else -> error("unknown session kind $name")
        }
        val completionSignals = stringArray(value["finiteCompletionSignals"])
        return GeminiLiveLifecyclePolicy(
            kind = kind,
            setupTimeoutMs = value.getValue("setupTimeoutMs").jsonPrimitive.long,
            firstResponseTimeoutMs = optionalLong(value, "firstResponseTimeoutMs"),
            contentIdleMs = optionalLong(value, "contentIdleMs"),
            hardResponseTimeoutMs = optionalLong(value, "hardResponseTimeoutMs"),
            serverIdleTimeoutMs = optionalLong(value, "serverIdleTimeoutMs"),
            serverIdleMinInputChunks = optionalLong(value, "serverIdleMinInputChunks") ?: 0,
            rotateAfterMs = optionalLong(value, "rotateAfterMs"),
            rotationQuietMs = optionalLong(value, "rotationQuietMs") ?: 0,
            reconnectEnabled = value.getValue("reconnectEnabled").jsonPrimitive.boolean,
            maxReconnectAttempts = optionalLong(value, "maxReconnectAttempts")?.toInt(),
            completeOnTurn = kind != GeminiLiveSessionKind.FINITE_REQUEST ||
                "turnComplete" in completionSignals,
            completeOnGeneration = kind != GeminiLiveSessionKind.FINITE_REQUEST ||
                "generationComplete" in completionSignals,
        )
    }

    private fun backoffPolicy(value: JsonObject): GeminiLiveBackoffPolicy {
        return GeminiLiveBackoffPolicy(
            baseMs = value.getValue("baseMs").jsonPrimitive.long,
            exponentCap = value.getValue("exponentCap").jsonPrimitive.long.toInt(),
            jitterMinMs = value.getValue("jitterMinMs").jsonPrimitive.long,
            jitterSeed = value.getValue("jitterSeed").jsonPrimitive.long,
            jitterStep = value.getValue("jitterStep").jsonPrimitive.long,
            jitterSpan = value.getValue("jitterSpan").jsonPrimitive.long,
            maxMs = value.getValue("maxMs").jsonPrimitive.long,
        )
    }

    private fun assertState(
        actual: GeminiLiveLifecycleState,
        expected: JsonObject,
        caseName: String,
    ) {
        val actualJson = stateJson(actual)
        expected.forEach { (field, expectedValue) ->
            assertEquals(
                "state field $field mismatch in $caseName",
                expectedValue,
                actualJson[field],
            )
        }
    }

    private fun stateJson(state: GeminiLiveLifecycleState): JsonObject = buildJsonObject {
        put("phase", phaseName(state.phase))
        put("generation", state.generation)
        putNullable("setupDeadlineMs", state.setupDeadlineMs)
        putNullable("firstResponseDeadlineMs", state.firstResponseDeadlineMs)
        putNullable("contentIdleDeadlineMs", state.contentIdleDeadlineMs)
        putNullable("hardResponseDeadlineMs", state.hardResponseDeadlineMs)
        putNullable("reconnectDeadlineMs", state.reconnectDeadlineMs)
        putNullable("goAwayDeadlineMs", state.goAwayDeadlineMs)
        put("reconnectAttempt", state.reconnectAttempt)
        put("hasOutput", state.hasOutput)
        put("inputChunksSinceServerActivity", state.inputChunksSinceServerActivity)
        putNullable("lastInputActivityMs", state.lastInputActivityMs)
        put("pendingWorkCount", state.pendingWorkCount)
        put("bufferedInputCount", state.bufferedInputCount)
        put("userSpeaking", state.userSpeaking)
        put("pendingToolIds", stringArrayJson(state.pendingToolIds))
    }

    private fun effectsJson(effects: List<GeminiLiveLifecycleEffect>): JsonArray {
        return buildJsonArray {
            effects.forEach { effect -> add(effectJson(effect)) }
        }
    }

    private fun effectJson(effect: GeminiLiveLifecycleEffect): JsonObject = buildJsonObject {
        when (effect) {
            is GeminiLiveLifecycleEffect.OpenSocket -> {
                put("type", "openSocket")
                put("generation", effect.generation)
            }
            is GeminiLiveLifecycleEffect.SendSetup -> {
                put("type", "sendSetup")
                put("generation", effect.generation)
            }
            is GeminiLiveLifecycleEffect.DeliverContent -> {
                put("type", "deliverContent")
                put("count", effect.count)
            }
            is GeminiLiveLifecycleEffect.FinalizeResponse -> {
                put("type", "finalizeResponse")
                put("reason", completionName(effect.reason))
            }
            GeminiLiveLifecycleEffect.FinalizeGeneration -> put("type", "finalizeGeneration")
            GeminiLiveLifecycleEffect.FinalizeTurn -> put("type", "finalizeTurn")
            GeminiLiveLifecycleEffect.StopPlayback -> put("type", "stopPlayback")
            GeminiLiveLifecycleEffect.DiscardQueuedOutput -> put("type", "discardQueuedOutput")
            GeminiLiveLifecycleEffect.FinalizeInterruptedGeneration -> {
                put("type", "finalizeInterruptedGeneration")
            }
            is GeminiLiveLifecycleEffect.DispatchTools -> {
                put("type", "dispatchTools")
                put("ids", stringArrayJson(effect.ids))
            }
            is GeminiLiveLifecycleEffect.CancelTools -> {
                put("type", "cancelTools")
                put("ids", stringArrayJson(effect.ids))
            }
            is GeminiLiveLifecycleEffect.CloseSocket -> {
                put("type", "closeSocket")
                put("generation", effect.generation)
            }
            is GeminiLiveLifecycleEffect.ScheduleReconnect -> {
                put("type", "scheduleReconnect")
                put("generation", effect.generation)
                put("attempt", effect.attempt)
                put("delayMs", effect.delayMs)
                put("reason", effect.reason.fixtureName)
            }
            is GeminiLiveLifecycleEffect.ReportFailure -> {
                put("type", "reportFailure")
                put("reason", effect.reason)
            }
            GeminiLiveLifecycleEffect.CancelSession -> put("type", "cancelSession")
        }
    }

    private fun phaseName(phase: GeminiLiveLifecyclePhase): String {
        return when (phase) {
            GeminiLiveLifecyclePhase.IDLE -> "idle"
            GeminiLiveLifecyclePhase.CONNECTING -> "connecting"
            GeminiLiveLifecyclePhase.AWAITING_SETUP -> "awaitingSetup"
            GeminiLiveLifecyclePhase.ACTIVE -> "active"
            GeminiLiveLifecyclePhase.BACKING_OFF -> "backingOff"
            GeminiLiveLifecyclePhase.COMPLETED -> "completed"
            GeminiLiveLifecyclePhase.CANCELLED -> "cancelled"
            GeminiLiveLifecyclePhase.FAILED -> "failed"
        }
    }

    private fun completionName(reason: GeminiLiveCompletionReason): String {
        return when (reason) {
            GeminiLiveCompletionReason.TURN_COMPLETE -> "turnComplete"
            GeminiLiveCompletionReason.GENERATION_COMPLETE -> "generationComplete"
            GeminiLiveCompletionReason.CONTENT_IDLE -> "contentIdle"
        }
    }

    private fun optionalLong(value: JsonObject, field: String): Long? {
        return (value[field] as? JsonPrimitive)?.longOrNull
    }

    private fun booleanField(value: JsonObject, field: String): Boolean {
        return (value[field] as? JsonPrimitive)?.booleanOrNull ?: false
    }

    private fun stringArray(value: JsonElement?): List<String> {
        return (value as? JsonArray)
            ?.map { item -> item.jsonPrimitive.content }
            .orEmpty()
    }

    private fun stringArrayJson(values: List<String>): JsonArray = buildJsonArray {
        values.forEach { value -> add(value) }
    }

    private fun kotlinx.serialization.json.JsonObjectBuilder.putNullable(
        field: String,
        value: Long?,
    ) {
        put(field, value?.let(::JsonPrimitive) ?: JsonNull)
    }

    private fun repoRoot(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.firstOrNull { root -> File(root, FIXTURE_PATH).exists() }
            ?: error("Could not locate $FIXTURE_PATH from $workingDirectory")
    }

    private companion object {
        private const val FIXTURE_PATH =
            "parity-fixtures/gemini-live-session/lifecycle.json"
    }
}
