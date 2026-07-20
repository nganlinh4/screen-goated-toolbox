package dev.screengoated.toolbox.mobile.phonecontrol

import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlEffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlGenerationId
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlJobId
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlJobStatus
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlSnapshotGeneration
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnDecisionCode
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnEffect
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnEvent
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnId
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnLifecycle
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnPhase
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnTransition
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlToolFramePreflight
import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlTurnLifecycleTest {
    @Test
    fun `turn lifecycle matches every shared fixture case and invariant`() {
        val fixture = Json.parseToJsonElement(
            File(repoRoot(), FIXTURE_PATH).readText(),
        ).jsonObject

        assertEquals(7L, fixture.requiredLong("schemaVersion"))
        assertEquals("phone-control", fixture.requiredString("feature"))
        assertEquals(
            PhoneControlTurnPhase.entries.map(PhoneControlTurnPhase::contractValue),
            fixture.getValue("states").jsonArray.map { state ->
                state.jsonPrimitive.content
            },
        )
        assertPhoneControlFixturePolicy(fixture.getValue("invariants").jsonObject)
        assertTrue(File(repoRoot(), fixture.requiredString("sharedSocketFixture")).isFile)

        fixture.getValue("cases").jsonArray.forEach { caseElement ->
            replayCase(caseElement.jsonObject)
        }
    }

    private fun replayCase(case: JsonObject) {
        val caseName = case.requiredString("name")
        val harness = FixtureHarness(phase(case.requiredString("start")))
        case.getValue("events").jsonArray.forEach { eventElement ->
            harness.apply(eventElement.jsonObject)
        }

        case.getValue("expect").jsonObject.forEach { (field, expected) ->
            assertEquals(
                "$field mismatch in $caseName",
                expected,
                contractElement(actualValue(field, harness)),
            )
        }
        assertTrue(
            "too many final responses in $caseName",
            harness.lifecycle.finalResponses <=
                harness.lifecycle.policy.maximumFinalResponsesPerUserTurn,
        )
    }

    private fun actualValue(field: String, harness: FixtureHarness): Any? {
        val lifecycle = harness.lifecycle
        val steps = harness.steps
        return when (field) {
            "state" -> lifecycle.phase.contractValue
            "finalResponses" -> lifecycle.finalResponses
            "firstToolCall" -> steps.ofType("toolCall")[0].decisionCode()
            "secondToolCall" -> steps.ofType("toolCall")[1].decisionCode()
            "maximumPendingJobsObserved" -> harness.maximumPendingJobsObserved
            "ownerReceiptDelivered" -> steps.last("toolReceipt")
                .hasEffect<PhoneControlTurnEffect.DeliverJobReceipt>()
            "laterToolDispatched" -> steps.ofType("toolCall")[1]
                .hasEffect<PhoneControlTurnEffect.DispatchJob>()
            "heldRejectionPeak" -> harness.heldRejectionPeak
            "protocolAbortAfterOwnerTerminal" -> harness.protocolAbortAfterOwnerTerminal
            "terminalDoneSuppressed" -> harness.terminalDoneSuppressed
            "overflowResponsesReplayed" -> harness.overflowResponsesReplayed
            "freshToolDispatched" -> steps.ofType("toolCall").last()
                .hasEffect<PhoneControlTurnEffect.DispatchJob>()
            "toolCallsDispatchedBeforeAbort" -> harness.toolCallsDispatchedBeforeAbort
            "protocolAbortBeforeResponse" -> harness.protocolAbortBeforeResponse
            "oldPayloadReplayed" -> harness.oldPayloadReplayed
            "freshSessionAccepted" -> harness.freshSessionAccepted
            "lateToolDispatched" -> steps.after("done", "toolCall")
                .hasEffect<PhoneControlTurnEffect.DispatchJob>()
            "lateAudioPlayed" -> steps.after("done", "audioChunk")
                .hasEffect<PhoneControlTurnEffect.PlayAudio>()
            "cleanupOutput" -> steps.last("cleanupComplete")
                .hasEffect<PhoneControlTurnEffect.FinalResponseReady>()
            "syntheticContinuation" -> steps.last("generationComplete")
                .hasEffect<PhoneControlTurnEffect.FinalGenerationRequested>()
            "finalGenerationCount" -> lifecycle.finalGenerationCount
            "oldJobCancelled" -> steps.last("bargeIn")
                .hasEffect<PhoneControlTurnEffect.CancelJob>()
            "oldReceiptCanAct" -> steps.retiredReceipt(lifecycle)
                .hasEffect<PhoneControlTurnEffect.DeliverJobReceipt>()
            "oldAudioPlayed" -> steps.retiredAudio(lifecycle)
                .hasEffect<PhoneControlTurnEffect.PlayAudio>()
            "reconciliationRequired" -> harness.reconciliationWasRequired
            "newTurnRemainsActive" -> lifecycle.turnRemainsActive
            "secondMutationBeforeObservation" -> steps.last("mutationRequested")
                .decisionCode()
            "firstDone" -> steps.ofType("done")[0].decisionCode()
            "secondDone" -> steps.ofType("done")[1].decisionCode()
            "resultCode" -> harness.lastResultCode
            "effectMayHaveOccurred" -> harness.lastEffectMayHaveOccurred
            "freshObservationRequired" -> lifecycle.freshObservationRequired
            "toolStillDeclaredNextNormalTurn" -> harness.toolStillDeclared
            "silentAlternateTool" -> harness.alternateToolSelected
            "requiredUserStepPresent" -> harness.requiredUserStepPresent
            "audioStartedBeforeToolReceipt" -> harness.audioStartedBeforeReceipt()
            "captionAndAudioGenerationMatch" -> harness.captionAndAudioMatch()
            "delayedReplay" -> harness.audioPlayedOutsideAudioEvent()
            "ambientScreenSentBeforeReceipt" -> harness.protocolSentBefore(
                "ambient_screen",
                "tool_response",
            )
            "microphoneAudioSentBeforeReceipt" -> harness.protocolSentBefore(
                "microphone_audio",
                "tool_response",
            )
            "toolResponseBeforeScreenEvidence" -> harness.protocolSentBefore(
                "tool_response",
                "tool_screen_evidence",
            )
            "screenEvidenceBeforeAmbientScreen" -> harness.protocolSentBefore(
                "tool_screen_evidence",
                "ambient_screen",
            )
            "retiredSocketAccepted" -> steps.retiredSocket(lifecycle)
                .hasEffect<PhoneControlTurnEffect.AcceptSocket>()
            "retiredToolDispatched" -> steps.retiredTool(lifecycle)
                .hasEffect<PhoneControlTurnEffect.DispatchJob>()
            "activeGeneration" -> lifecycle.activeGeneration?.value
            "authenticatedSessionPreserved" -> harness.authenticatedSessionPreserved
            "credentialsCopied" -> harness.credentialsCopied
            "genericDomAuthorityClaimed" -> harness.genericDomAuthorityClaimed
            "requestedToolChanged" -> harness.requestedToolChanged
            "provider" -> harness.reportedProvider
            "providerReported" -> harness.providerReported
            "platformDispatchPerformed" -> harness.platformDispatchPerformed
            "terminalCancellationAcknowledged" -> harness.terminalCancellationAcknowledged
            "newMutationDispatchedBeforeProviderTerminal" ->
                harness.newMutationDispatchedBeforeProviderTerminal
            "terminalCancellationAcknowledgedAfterProviderTerminal" ->
                harness.terminalCancellationAcknowledgedAfterProviderTerminal
            "freshObservationRequiredBeforeNewMutation" ->
                harness.freshObservationRequiredBeforeNewMutation
            else -> error("Unsupported Phone Control expectation field: $field")
        }
    }

    private inner class FixtureHarness(start: PhoneControlTurnPhase) {
        val lifecycle = PhoneControlTurnLifecycle(start)
        val steps = mutableListOf<ReducedStep>()

        private val declaredTools = mutableSetOf<String>()
        private val observedJobIds = mutableSetOf<PhoneControlJobId>()
        private var lastRequestedTool: String? = null
        private var credentialContext: String? = null
        private var semanticRequestedTool: String? = null
        private var routedRequestedTool: String? = null
        private var matchingCdpTargetFound = false
        private val ownedEffectBoundaryJobs = mutableSetOf<PhoneControlJobId>()
        private val acceptedEffectJobs = mutableSetOf<PhoneControlJobId>()
        private val providerTerminalJobs = mutableSetOf<PhoneControlJobId>()
        private val cancelledJobs = mutableSetOf<PhoneControlJobId>()
        private val protocolSends = mutableListOf<String>()
        private var synchronousToolOutstanding = false
        private var deferredAmbientScreen = false

        var lastResultCode: String? = null
            private set
        var lastEffectMayHaveOccurred: Boolean? = null
            private set
        var alternateToolSelected: Boolean = false
            private set
        var requiredUserStepPresent: Boolean = false
            private set
        var authenticatedSessionPreserved: Boolean = false
            private set
        val credentialsCopied: Boolean = false
        var genericDomAuthorityClaimed: Boolean = false
            private set
        var reportedProvider: String? = null
            private set
        var providerReported: Boolean = false
            private set
        var maximumPendingJobsObserved: Int = 0
            private set
        var heldRejectionPeak: Int = 0
            private set
        var protocolAbortAfterOwnerTerminal: Boolean = false
            private set
        var terminalDoneSuppressed: Boolean = false
            private set
        val overflowResponsesReplayed: Boolean = false
        private var rejectionOverflowLatched: Boolean = false
        private var overflowOwnerTool: String? = null
        var toolCallsDispatchedBeforeAbort = 0
            private set
        var protocolAbortBeforeResponse = false
            private set
        var oldPayloadReplayed = false
            private set
        var freshSessionAccepted = false
            private set
        private var oldPayloadQueued = false
        var reconciliationWasRequired = false
            private set
        var platformDispatchPerformed = false
            private set
        var terminalCancellationAcknowledged = false
            private set
        var newMutationDispatchedBeforeProviderTerminal = false
            private set
        var terminalCancellationAcknowledgedAfterProviderTerminal = false
            private set
        var freshObservationRequiredBeforeNewMutation = false
            private set

        val toolStillDeclared: Boolean
            get() = lastRequestedTool?.let(declaredTools::contains) == true

        val requestedToolChanged: Boolean
            get() = routedRequestedTool != semanticRequestedTool

        fun apply(raw: JsonObject) {
            val type = raw.requiredString("type")
            val event = lifecycleEvent(raw)
            val transition = event?.let(lifecycle::reduce)
            (event as? PhoneControlTurnEvent.JobRequested)?.jobId?.let(observedJobIds::add)
            transition?.effects?.filterIsInstance<PhoneControlTurnEffect.CancelJob>()
                ?.mapTo(cancelledJobs, PhoneControlTurnEffect.CancelJob::jobId)
            if (transition?.effects?.any {
                    it is PhoneControlTurnEffect.ReconciliationRequired
                } == true
            ) {
                reconciliationWasRequired = true
            }
            if (event is PhoneControlTurnEvent.JobRequested &&
                acceptedEffectJobs.any { it !in providerTerminalJobs }
            ) {
                newMutationDispatchedBeforeProviderTerminal =
                    newMutationDispatchedBeforeProviderTerminal ||
                    transition?.effects?.any { it is PhoneControlTurnEffect.DispatchJob } == true
            }
            maximumPendingJobsObserved = maxOf(
                maximumPendingJobsObserved,
                observedJobIds.count { id ->
                    lifecycle.job(id)?.status == PhoneControlJobStatus.PENDING
                },
            )
            applyExternalContract(type, raw)
            if (transition?.decision == PhoneControlTurnDecisionCode.STALE_TARGET) {
                lastResultCode = transition.decision.contractValue
                lastEffectMayHaveOccurred = false
            }
            if (type == "toolReceipt" && rejectionOverflowLatched) {
                protocolAbortAfterOwnerTerminal = true
                terminalDoneSuppressed = overflowOwnerTool == "done"
            }
            steps += ReducedStep(type, event, transition)
        }

        fun audioStartedBeforeReceipt(): Boolean {
            val audioIndex = steps.indexOfFirst { step ->
                step.hasEffect<PhoneControlTurnEffect.PlayAudio>()
            }
            val receiptIndex = steps.indexOfFirst { step ->
                step.hasEffect<PhoneControlTurnEffect.DeliverJobReceipt>()
            }
            return audioIndex >= 0 && receiptIndex >= 0 && audioIndex < receiptIndex
        }

        fun captionAndAudioMatch(): Boolean {
            val audio = steps.flatMap { step ->
                step.effects<PhoneControlTurnEffect.PlayAudio>().map { it.chunk }
            }
            val captions = steps.flatMap { step ->
                step.effects<PhoneControlTurnEffect.DeliverCaption>().map { it.chunk }
            }
            return audio.any(captions::contains)
        }

        fun audioPlayedOutsideAudioEvent(): Boolean {
            return steps.any { step ->
                step.type != "audioChunk" &&
                    step.hasEffect<PhoneControlTurnEffect.PlayAudio>()
            }
        }

        fun protocolSentBefore(first: String, second: String): Boolean {
            val firstIndex = protocolSends.indexOf(first)
            val secondIndex = protocolSends.indexOf(second)
            return firstIndex >= 0 && secondIndex >= 0 && firstIndex < secondIndex
        }

        private fun applyExternalContract(type: String, raw: JsonObject) {
            when (type) {
                "toolCall" -> synchronousToolOutstanding = true
                "ambientScreenFrame" -> {
                    if (synchronousToolOutstanding) deferredAmbientScreen = true
                    else protocolSends += "ambient_screen"
                }
                "microphoneAudio" -> protocolSends += "microphone_audio"
                "toolReceipt" -> {
                    protocolSends += "tool_response"
                    if (raw.optionalBoolean("screenEvidence") == true) {
                        protocolSends += "tool_screen_evidence"
                    }
                    synchronousToolOutstanding = false
                    if (deferredAmbientScreen) {
                        protocolSends += "ambient_screen"
                        deferredAmbientScreen = false
                    }
                }
                "toolRequested" -> {
                    lastRequestedTool = raw.requiredString("tool")
                    declaredTools += requireNotNull(lastRequestedTool)
                }
                "providerState" -> {
                    val state = raw.requiredString("state")
                    lastResultCode = if (state == "ready") {
                        PhoneControlTurnDecisionCode.ACCEPTED.contractValue
                    } else {
                        "capability_unavailable"
                    }
                    alternateToolSelected = false
                    requiredUserStepPresent = state == "needs_user_step"
                }
                "browserNavigationRequested" -> {
                    credentialContext = raw.requiredString("credentialContext")
                }
                "customTabOpened" -> {
                    authenticatedSessionPreserved =
                        raw.requiredBoolean("sharesPreferredBrowserState") &&
                        credentialContext == "preferred_browser"
                }
                "semanticActionRequested" -> {
                    semanticRequestedTool = raw.requiredString("requestedTool")
                    routedRequestedTool = semanticRequestedTool
                }
                "cdpTargetProbe" -> {
                    matchingCdpTargetFound = raw.requiredBoolean("matchingTargetFound")
                    genericDomAuthorityClaimed = false
                }
                "providerRoute" -> {
                    reportedProvider = raw.requiredString("provider")
                    providerReported = true
                    genericDomAuthorityClaimed =
                        raw.requiredString("capability") == "browser_semantic" &&
                        reportedProvider == "browser_cdp" &&
                        matchingCdpTargetFound
                }
                "rejectionFlood" -> {
                    val rejectedCount = raw.requiredLong("rejectedCount").toInt()
                    heldRejectionPeak = minOf(
                        rejectedCount,
                        lifecycle.policy.maximumHeldToolRejections,
                    )
                    rejectionOverflowLatched =
                        rejectedCount > lifecycle.policy.maximumHeldToolRejections
                    overflowOwnerTool = raw.requiredString("ownerTool")
                }
                "toolFrameOverflow" -> {
                    if (raw.requiredLong("callCount") > PhoneControlToolFramePreflight.MAXIMUM_CALLS) {
                        protocolAbortBeforeResponse = true
                        toolCallsDispatchedBeforeAbort = 0
                    }
                }
                "queuedControlPayload" -> oldPayloadQueued = true
                "sessionReconnect" -> {
                    oldPayloadReplayed = oldPayloadQueued &&
                        raw.requiredString("resumptionHandle").isNotBlank()
                }
                "freshProtocolSession" -> freshSessionAccepted = protocolAbortBeforeResponse
                "ownedEffectBoundary" -> {
                    ownedEffectBoundaryJobs += PhoneControlJobId(raw.requiredString("jobId"))
                }
                "platformDispatchAttempt" -> {
                    val jobId = PhoneControlJobId(raw.requiredString("jobId"))
                    platformDispatchPerformed = jobId in ownedEffectBoundaryJobs &&
                        jobId !in cancelledJobs
                    if (!platformDispatchPerformed &&
                        jobId in ownedEffectBoundaryJobs &&
                        jobId !in acceptedEffectJobs
                    ) {
                        lifecycle.reduce(
                            PhoneControlTurnEvent.JobReceipt(
                                generation = raw.requiredGeneration("generation"),
                                jobId = jobId,
                                certainty = PhoneControlEffectCertainty.PROVEN_NO_EFFECT,
                            ),
                        )
                        terminalCancellationAcknowledged = true
                        lastEffectMayHaveOccurred = false
                    }
                }
                "platformEffectAccepted" -> {
                    acceptedEffectJobs += PhoneControlJobId(raw.requiredString("jobId"))
                }
                "providerTerminalCallback" -> {
                    val jobId = PhoneControlJobId(raw.requiredString("jobId"))
                    providerTerminalJobs += jobId
                    val terminal = lifecycle.reduce(
                        PhoneControlTurnEvent.JobReceipt(
                            generation = raw.requiredGeneration("generation"),
                            jobId = jobId,
                            certainty = PhoneControlEffectCertainty.MAY_HAVE_OCCURRED,
                        ),
                    )
                    reconciliationWasRequired = reconciliationWasRequired ||
                        terminal.effects.any {
                            it is PhoneControlTurnEffect.ReconciliationRequired
                        }
                    freshObservationRequiredBeforeNewMutation =
                        lifecycle.reconciliationRequired
                    terminalCancellationAcknowledgedAfterProviderTerminal = true
                }
            }
        }
    }

    private fun lifecycleEvent(value: JsonObject): PhoneControlTurnEvent? {
        return when (val type = value.requiredString("type")) {
            "toolReceipt" -> PhoneControlTurnEvent.JobReceipt(
                generation = value.optionalGeneration("generation"),
                jobId = value.optionalString("jobId")?.let(::PhoneControlJobId),
                certainty = value.certainty(),
            )
            "done" -> PhoneControlTurnEvent.TerminalDone(
                generation = value.optionalGeneration("generation"),
                assistantContentSeen = value.optionalBoolean("assistantContentSeen"),
            )
            "toolCall" -> PhoneControlTurnEvent.JobRequested(
                generation = value.requiredGeneration("generation"),
                jobId = value.optionalString("jobId")?.let(::PhoneControlJobId),
            )
            "audioChunk" -> PhoneControlTurnEvent.AudioReceived(value.outputChunk())
            "captionChunk" -> PhoneControlTurnEvent.CaptionReceived(value.outputChunk())
            "cleanupComplete" -> PhoneControlTurnEvent.CleanupCompleted(
                generation = value.optionalGeneration("generation"),
            )
            "assistantContent" -> PhoneControlTurnEvent.AssistantContentReceived(
                generation = value.requiredGeneration("generation"),
            )
            "generationComplete" -> PhoneControlTurnEvent.GenerationCompleted(
                generation = value.requiredGeneration("generation"),
            )
            "bargeIn" -> value.requiredLong("newTurn").let { id ->
                PhoneControlTurnEvent.UserBargeIn(
                    newTurn = PhoneControlTurnId(id),
                    newGeneration = PhoneControlGenerationId(id),
                )
            }
            "mutationInterrupted" -> PhoneControlTurnEvent.MutationInterrupted(
                certainty = value.certainty(),
            )
            "mutationRequested" -> PhoneControlTurnEvent.MutationRequested
            "freshObservation" -> PhoneControlTurnEvent.FreshObservation(
                stateReconciled = value.requiredBoolean("stateReconciled"),
            )
            "observe" -> PhoneControlTurnEvent.TargetObserved(value.targetIdentity())
            "surfaceChanged" -> PhoneControlTurnEvent.SurfaceChanged(
                PhoneControlSnapshotGeneration(value.requiredLong("snapshotGeneration")),
            )
            "actionRequested" -> PhoneControlTurnEvent.TargetActionRequested(
                value.targetIdentity(),
            )
            "transportFailure" -> PhoneControlTurnEvent.TransportFailed(
                value.requiredGeneration("generation"),
            )
            "socketOpened" -> PhoneControlTurnEvent.SocketOpened(
                value.requiredGeneration("generation"),
            )
            in EXTERNAL_EVENT_TYPES -> null
            else -> error("Unknown Phone Control turn event: $type")
        }
    }

    private fun phase(value: String): PhoneControlTurnPhase {
        return PhoneControlTurnPhase.entries.singleOrNull { phase ->
            phase.contractValue == value
        } ?: error("Unknown Phone Control turn phase: $value")
    }

    private fun List<ReducedStep>.ofType(type: String): List<ReducedStep> {
        return filter { step -> step.type == type }
    }

    private fun List<ReducedStep>.last(type: String): ReducedStep {
        return lastOrNull { step -> step.type == type }
            ?: error("Missing fixture event: $type")
    }

    private fun List<ReducedStep>.after(anchor: String, type: String): ReducedStep {
        val anchorIndex = indexOfFirst { step -> step.type == anchor }
        return drop(anchorIndex + 1).firstOrNull { step -> step.type == type }
            ?: error("Missing $type after $anchor")
    }

    private fun List<ReducedStep>.jobId(type: String): String {
        val event = last(type).event as PhoneControlTurnEvent.JobRequested
        return event.jobId?.value ?: error("Missing job id on $type")
    }

    private fun List<ReducedStep>.retiredReceipt(
        lifecycle: PhoneControlTurnLifecycle,
    ): ReducedStep {
        return first { step ->
            val generation = (step.event as? PhoneControlTurnEvent.JobReceipt)?.generation
            generation?.let(lifecycle::isGenerationRetired) == true
        }
    }

    private fun List<ReducedStep>.retiredAudio(
        lifecycle: PhoneControlTurnLifecycle,
    ): ReducedStep {
        return first { step ->
            val generation = (step.event as? PhoneControlTurnEvent.AudioReceived)
                ?.chunk
                ?.generation
            generation?.let(lifecycle::isGenerationRetired) == true
        }
    }

    private fun List<ReducedStep>.retiredSocket(
        lifecycle: PhoneControlTurnLifecycle,
    ): ReducedStep {
        return first { step ->
            val generation = (step.event as? PhoneControlTurnEvent.SocketOpened)?.generation
            generation?.let(lifecycle::isGenerationRetired) == true
        }
    }

    private fun List<ReducedStep>.retiredTool(
        lifecycle: PhoneControlTurnLifecycle,
    ): ReducedStep {
        return first { step ->
            val generation = (step.event as? PhoneControlTurnEvent.JobRequested)?.generation
            generation?.let(lifecycle::isGenerationRetired) == true
        }
    }

    private fun ReducedStep.decisionCode(): String? {
        return transition?.decision?.contractValue
    }

    private inline fun <reified T : PhoneControlTurnEffect> ReducedStep.hasEffect(): Boolean {
        return transition?.effects?.any { effect -> effect is T } == true
    }

    private inline fun <reified T : PhoneControlTurnEffect> ReducedStep.effects(): List<T> {
        return transition?.effects?.filterIsInstance<T>().orEmpty()
    }

    private fun repoRoot(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.firstOrNull { root -> File(root, FIXTURE_PATH).isFile }
            ?: error("Could not locate $FIXTURE_PATH from $workingDirectory")
    }

    private data class ReducedStep(
        val type: String,
        val event: PhoneControlTurnEvent?,
        val transition: PhoneControlTurnTransition?,
    )

    private companion object {
        private const val FIXTURE_PATH =
            "parity-fixtures/phone-control/turn-contract.json"

        private val EXTERNAL_EVENT_TYPES = setOf(
            "toolRequested",
            "providerState",
            "browserNavigationRequested",
            "customTabOpened",
            "semanticActionRequested",
            "cdpTargetProbe",
            "providerRoute",
            "rejectionFlood",
            "toolFrameOverflow",
            "queuedControlPayload",
            "sessionReconnect",
            "freshProtocolSession",
            "ownedEffectBoundary",
            "platformDispatchAttempt",
            "platformEffectAccepted",
            "providerTerminalCallback",
            "ambientScreenFrame",
            "microphoneAudio",
        )
    }
}
