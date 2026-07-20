package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlEffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlGenerationId
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlJobId
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlOutputChunk
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnEffect
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnEvent
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnId
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnLifecycle
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnDecisionCode
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnPhase
import dev.screengoated.toolbox.mobile.phonecontrol.session.buildPhoneControlToolResponse
import dev.screengoated.toolbox.mobile.phonecontrol.tools.PhoneControlToolRegistry
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveFunctionCall
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleEffect
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveServerFrame
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.Channel
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal class PhoneControlTurnCoordinator(
    executor: PhoneControlToolExecutor,
    scope: CoroutineScope,
    private val sink: PhoneControlTurnSink,
    private val recorder: PhoneControlTurnRecorder = NoOpPhoneControlTurnRecorder,
) {
    private val completionEvents = Channel<PhoneControlToolCompletionEvent>(
        capacity = PHONE_CONTROL_COMPLETION_QUEUE_CAPACITY,
    )
    private val tools = PhoneControlToolController(executor, scope, completionEvents)
    private val lifecycle = PhoneControlTurnLifecycle(PhoneControlTurnPhase.LISTENING)
    private val inputTranscript = PhoneControlInputTranscriptAssembler()
    private val assistantTranscript = PhoneControlTranscriptAccumulator()
    private val heldToolRejections = PhoneControlHeldToolRejections()
    private val outbound = PhoneControlTurnOutbound(sink)

    private var nextTurn = 0L
    private var nextGeneration = 0L
    private var nextOutputSequence = 0L
    private var currentGeneration: PhoneControlGenerationId? = null
    private var finalGeneration: PhoneControlGenerationId? = null
    private var terminalDone = false
    private var terminalSummary: String? = null
    private var pendingTerminalDone: PhoneControlCompletedTool? = null
    private var generationAwaitingReconciliation: PhoneControlGenerationId? = null

    val pendingWorkCount: Int
        get() = tools.pendingCount

    val heldRejectionCount: Int
        get() = heldToolRejections.size

    val phase: PhoneControlTurnPhase
        get() = lifecycle.phase

    fun handleFrame(
        frame: GeminiLiveServerFrame,
        effects: List<GeminiLiveLifecycleEffect>,
    ) {
        if (outbound.refused) return
        val interrupted = effects.any {
            it == GeminiLiveLifecycleEffect.FinalizeInterruptedGeneration
        }
        if (interrupted) {
            sink.interruptPlayback()
            interruptGeneration()
            inputTranscript.beginEpoch()
        }
        mergeInputTranscript(frame.inputTranscript)
        val frameGeneration = if (interrupted) null else generationForFrame(frame)
        effects.forEach { effect ->
            when (effect) {
                is GeminiLiveLifecycleEffect.DeliverContent ->
                    if (!interrupted) deliverContent(frame, frameGeneration)
                is GeminiLiveLifecycleEffect.DispatchTools ->
                    dispatchTools(frame.toolCalls, effect.ids, frameGeneration)
                GeminiLiveLifecycleEffect.StopPlayback,
                GeminiLiveLifecycleEffect.DiscardQueuedOutput,
                GeminiLiveLifecycleEffect.FinalizeInterruptedGeneration,
                -> if (!interrupted) applyLivePlaybackEffect(effect)
                is GeminiLiveLifecycleEffect.CancelTools -> cancelTools(effect.ids)
                GeminiLiveLifecycleEffect.FinalizeGeneration,
                GeminiLiveLifecycleEffect.FinalizeTurn,
                is GeminiLiveLifecycleEffect.FinalizeResponse,
                -> if (!interrupted) completeGeneration(frameGeneration)
                else -> Unit
            }
        }
    }

    fun userSpeechStarted(assistantPlaybackActive: Boolean) {
        if (!assistantPlaybackActive) inputTranscript.beginEpoch()
    }

    fun drainToolCompletions() {
        while (true) {
            val event = completionEvents.tryReceive().getOrNull() ?: break
            val completed = tools.takeCompletion(event) ?: continue
            val generation = PhoneControlGenerationId(completed.request.generation)
            val jobId = PhoneControlJobId(completed.request.id)
            Log.i(TAG, completed.structuralReceiptLog())
            val transition = lifecycle.reduce(
                PhoneControlTurnEvent.JobReceipt(
                    generation = generation,
                    jobId = jobId,
                    certainty = completed.result.certainty,
                ),
            )
            if (transition.effects.any { it is PhoneControlTurnEffect.DeliverJobReceipt }) {
                if (completed.result.response.stateReconciled()) {
                    lifecycle.reduce(PhoneControlTurnEvent.FreshObservation(stateReconciled = true))
                }
                if (completed.request.name == DONE_TOOL_NAME &&
                    completed.result.terminalSummary != null
                ) {
                    if (pendingTerminalDone == null) {
                        pendingTerminalDone = completed
                    } else {
                        outbound.sendPayload(
                            buildPhoneControlToolResponse(
                                id = completed.request.id,
                                name = completed.request.name,
                                response = buildJsonObject {
                                    put("code", "duplicate_terminal")
                                    put("message", "A terminal completion is already pending.")
                                    put("effect", "proven_no_effect")
                                },
                            ),
                        )
                    }
                } else {
                    deliverToolReceipt(completed)
                }
            } else {
                Log.d(TAG, "absorbed_tool_receipt id=${completed.request.id} decision=${transition.decision.contractValue}")
            }
            if (lifecycle.reconciliationRequired) sink.reconciliationRequired()
        }
        if (abortOverflowedSessionIfSettled()) return
        flushHeldToolRejections()
        finishPendingTerminalDone()
    }

    fun abandonProtocolSession() {
        outbound.block()
        sink.interruptPlayback()
        sink.discardQueuedPlayback()
        interruptGeneration()
        tools.cancelAll()
        heldToolRejections.reset()
        pendingTerminalDone = null
    }

    fun freshProtocolSessionBound() = outbound.reset()

    fun freshScreenEvidenceDelivered() {
        if (!lifecycle.reconciliationRequired) return
        lifecycle.reduce(PhoneControlTurnEvent.FreshObservation(stateReconciled = true))
        Log.i(TAG, "reconciliation_cleared source=screen_frame")
        finishPendingTerminalDone()
        generationAwaitingReconciliation?.let(::completeGeneration)
    }

    fun stop() {
        tools.cancelAll()
        heldToolRejections.reset()
        completionEvents.close()
    }

    private fun mergeInputTranscript(fragment: String?) {
        val update = fragment?.let(inputTranscript::merge) ?: return
        if (update.startsTurn) activateNewTurn()
        if (update.changed) {
            sink.updateInputCaption(update.text)
            lifecycle.activeTurn?.let { turn ->
                recorder.userTranscriptUpdated(turn.value, update.text)
            }
        }
    }

    private fun generationForFrame(frame: GeminiLiveServerFrame): PhoneControlGenerationId? {
        currentGeneration?.let { return it }
        if (terminalDone && lifecycle.phase != PhoneControlTurnPhase.FINALIZING) return null
        val needsGeneration = frame.contentCount > 0 ||
            frame.toolCalls.isNotEmpty() ||
            frame.responseComplete ||
            frame.interrupted
        return if (needsGeneration && nextTurn == 0L) activateNewTurn() else null
    }

    private fun activateNewTurn(): PhoneControlGenerationId {
        // A newly admitted user turn owns playback immediately. Stop and retire
        // any audio that was still draining from the preceding turn before the
        // new generation can enqueue output.
        sink.interruptPlayback()
        sink.discardQueuedPlayback()
        heldToolRejections.discardHeld()
        val replacedTurn = lifecycle.activeTurn?.takeIf { lifecycle.turnRemainsActive }
        nextTurn = nextOrdinal(nextTurn)
        nextGeneration = nextOrdinal(nextGeneration)
        val turn = PhoneControlTurnId(nextTurn)
        val generation = PhoneControlGenerationId(nextGeneration)
        inputTranscript.claimCurrentEpoch()
        assistantTranscript.reset()
        val transition = lifecycle.reduce(PhoneControlTurnEvent.UserBargeIn(turn, generation))
        applyTurnEffects(transition.effects)
        replacedTurn?.let { recorder.turnInterrupted(it.value) }
        currentGeneration = generation
        finalGeneration = null
        terminalDone = false
        terminalSummary = null
        pendingTerminalDone = null
        generationAwaitingReconciliation = null
        recorder.turnStarted(turn.value, generation.value)
        sink.updateOrbPresentation(orbThinkingPresentation.stateLabel, null)
        sink.updateOutputCaption("")
        sink.updateTurnPhase(PhoneControlTurnPhase.WORKING)
        return generation
    }

    private fun interruptGeneration() {
        val generation = currentGeneration ?: return
        val turn = lifecycle.activeTurn
        val transition = lifecycle.reduce(
            PhoneControlTurnEvent.GenerationInterrupted(generation),
        )
        applyTurnEffects(transition.effects)
        if (transition.decision == PhoneControlTurnDecisionCode.ACCEPTED) {
            turn?.let { recorder.turnInterrupted(it.value) }
        }
        currentGeneration = null
        finalGeneration = null
        terminalDone = false
        terminalSummary = null
        pendingTerminalDone = null
        generationAwaitingReconciliation = null
        heldToolRejections.discardHeld()
        sink.updateTurnPhase(PhoneControlTurnPhase.LISTENING)
    }

    private fun applyLivePlaybackEffect(effect: GeminiLiveLifecycleEffect) {
        when (effect) {
            GeminiLiveLifecycleEffect.StopPlayback -> sink.interruptPlayback()
            GeminiLiveLifecycleEffect.DiscardQueuedOutput -> sink.discardQueuedPlayback()
            else -> Unit
        }
    }

    private fun deliverContent(
        frame: GeminiLiveServerFrame,
        generation: PhoneControlGenerationId?,
    ) {
        val assistantContent = frame.outputTranscript?.isNotBlank() == true ||
            frame.visibleTextParts.isNotEmpty() ||
            frame.audioParts.isNotEmpty()
        if (!assistantContent || generation == null) return
        sink.updateOrbPresentation(orbRespondingPresentation.stateLabel, null)

        val contentTransition = lifecycle.reduce(
            PhoneControlTurnEvent.AssistantContentReceived(generation),
        )
        if (contentTransition.decision != PhoneControlTurnDecisionCode.ACCEPTED) return
        sink.updateTurnPhase(lifecycle.phase)

        val caption = frame.outputTranscript?.takeIf(String::isNotBlank)
            ?: frame.visibleTextParts.joinToString(separator = "").takeIf(String::isNotBlank)
        caption?.let { text ->
            val chunk = outputChunk(generation)
            val transition = lifecycle.reduce(PhoneControlTurnEvent.CaptionReceived(chunk))
            if (transition.effects.any { it is PhoneControlTurnEffect.DeliverCaption }) {
                if (assistantTranscript.merge(text)) {
                    sink.updateOutputCaption(assistantTranscript.text)
                    if (lifecycle.turnRemainsActive) {
                        lifecycle.activeTurn?.let { turn ->
                            recorder.assistantTranscriptUpdated(turn.value, assistantTranscript.text)
                        }
                    }
                }
            }
        }
        frame.audioParts.forEach { inline ->
            val bytes = decodePhoneControlPcm24k(inline.mimeType, inline.data) ?: return@forEach
            val chunk = outputChunk(generation)
            val transition = lifecycle.reduce(PhoneControlTurnEvent.AudioReceived(chunk))
            if (transition.effects.any { it is PhoneControlTurnEffect.PlayAudio }) {
                sink.playAudio(bytes)
            }
        }
    }

    private fun dispatchTools(
        calls: List<GeminiLiveFunctionCall>,
        ids: List<String>,
        generation: PhoneControlGenerationId?,
    ) {
        val preflightRejection = PhoneControlToolFramePreflight.rejection(calls)
        if (preflightRejection != null) {
            heldToolRejections.latchOverflow(generation)
            Log.e(
                TAG,
                "tool_frame_rejected reason=${preflightRejection.name.lowercase()} " +
                    "calls=${calls.size} generation=${generation?.value ?: -1L}",
            )
            abortOverflowedSessionIfSettled()
            return
        }
        val requested = ids.toSet()
        calls.filter { it.id in requested }.forEach { call ->
            if (heldToolRejections.overflowed || outbound.refused) return@forEach
            if (call.id.isBlank() || call.name.isBlank()) {
                sendRejectedCall(
                    call,
                    generation,
                    "invalid_tool_call",
                    "Tool id and name are required.",
                )
            } else if (generation == null ||
                terminalDone ||
                pendingTerminalDone != null ||
                lifecycle.phase == PhoneControlTurnPhase.FINALIZING
            ) {
                sendRejectedCall(call, generation, "turn_closed", "The user turn is already closed.")
            } else if (call.name == DONE_TOOL_NAME && lifecycle.reconciliationRequired) {
                sendRejectedCall(
                    call,
                    generation,
                    "blocked_reconciliation_required",
                    "Observe the current state before closing the turn.",
                )
            } else if (!authorizeMutation(call)) {
                sendRejectedCall(
                    call,
                    generation,
                    "blocked_reconciliation_required",
                    "Observe the current state before dispatching another mutation.",
                )
            } else {
                dispatchTool(call, generation)
            }
        }
    }

    private fun dispatchTool(
        call: GeminiLiveFunctionCall,
        generation: PhoneControlGenerationId,
    ) {
        val jobId = PhoneControlJobId(call.id)
        Log.i(TAG, call.structuralDispatchLog(generation.value))
        call.orbPresentation().let { sink.updateOrbPresentation(it.stateLabel, it.iconOverride) }
        val transition = lifecycle.reduce(PhoneControlTurnEvent.JobRequested(generation, jobId))
        if (transition.effects.none { it is PhoneControlTurnEffect.DispatchJob }) {
            if (transition.decision == PhoneControlTurnDecisionCode.DUPLICATE_EVENT) return
            val priorActionSettling =
                transition.decision == PhoneControlTurnDecisionCode.TOOL_CALL_IN_FLIGHT &&
                    tools.activeRequest?.generation != generation.value
            val code = if (priorActionSettling) "prior_action_settling"
            else transition.decision.contractValue
            sendRejectedCall(
                call,
                generation,
                code,
                "The tool call was not admitted.",
            )
            return
        }
        val turnId = lifecycle.activeTurn?.value ?: 0L
        val admission = tools.dispatch(
            PhoneControlToolRequest(
                id = call.id,
                name = call.name,
                arguments = call.args,
                turnId = turnId,
                generation = generation.value,
            ),
        )
        if (admission != PhoneControlToolAdmission.ACCEPTED) {
            lifecycle.reduce(
                PhoneControlTurnEvent.JobReceipt(
                    generation = generation,
                    jobId = jobId,
                    certainty = PhoneControlEffectCertainty.PROVEN_NO_EFFECT,
                ),
            )
            val active = tools.activeRequest
            val code = when {
                admission == PhoneControlToolAdmission.DUPLICATE_ID -> "duplicate_tool_call"
                active != null && active.generation != generation.value -> "prior_action_settling"
                else -> "tool_call_in_flight"
            }
            sendRejectedCall(
                call,
                generation,
                code,
                "Another admitted tool job must settle before this call can run.",
            )
        }
        sink.updateTurnPhase(PhoneControlTurnPhase.WORKING)
    }

    private fun completeTerminalDone(
        generation: PhoneControlGenerationId,
        summary: String,
    ): PhoneControlTurnDecisionCode {
        val turn = lifecycle.activeTurn
        val contentSeen = lifecycle.generation(generation)?.assistantContentSeen == true
        val transition = lifecycle.reduce(
            PhoneControlTurnEvent.TerminalDone(generation, contentSeen),
        )
        if (transition.decision != PhoneControlTurnDecisionCode.ACCEPTED) {
            return transition.decision
        }
        terminalDone = true
        terminalSummary = summary
        sink.updateOrbPresentation(orbDonePresentation.stateLabel, null)
        if (transition.effects.any { it is PhoneControlTurnEffect.FinalGenerationRequested }) {
            nextGeneration = nextOrdinal(nextGeneration)
            finalGeneration = PhoneControlGenerationId(nextGeneration)
            currentGeneration = finalGeneration
            sink.updateTurnPhase(PhoneControlTurnPhase.FINALIZING)
        } else {
            currentGeneration = null
            recordCompletedTurn(turn)
            sink.updateTurnPhase(PhoneControlTurnPhase.IDLE)
        }
        return transition.decision
    }

    private fun completeGeneration(generation: PhoneControlGenerationId?) {
        if (generation == null) return
        if (tools.pendingCount > 0 || pendingTerminalDone != null) {
            sink.updateTurnPhase(PhoneControlTurnPhase.WORKING)
            return
        }
        if (lifecycle.reconciliationRequired) {
            generationAwaitingReconciliation = generation
            sink.reconciliationRequired()
            sink.updateTurnPhase(PhoneControlTurnPhase.WORKING)
            return
        }
        if (generationAwaitingReconciliation == generation) {
            generationAwaitingReconciliation = null
        }
        val turn = lifecycle.activeTurn
        val transition = lifecycle.reduce(PhoneControlTurnEvent.GenerationCompleted(generation))
        applyTurnEffects(transition.effects)
        if (currentGeneration == generation) currentGeneration = null
        if (finalGeneration == generation) finalGeneration = null
        if (transition.effects.any { it is PhoneControlTurnEffect.FinalResponseReady }) {
            recordCompletedTurn(turn)
            sink.updateTurnPhase(PhoneControlTurnPhase.IDLE)
        }
    }

    private fun recordCompletedTurn(turn: PhoneControlTurnId?) {
        turn ?: return
        val assistant = assistantTranscript.text.ifBlank { terminalSummary.orEmpty() }
        recorder.turnCompleted(turn.value, inputTranscript.text, assistant)
    }

    private fun cancelTools(ids: List<String>) {
        tools.cancel(ids)
    }

    private fun authorizeMutation(call: GeminiLiveFunctionCall): Boolean {
        val mutating = PhoneControlToolRegistry.byName[call.name]?.handler?.mutating == true
        if (!mutating) return true
        return lifecycle.reduce(PhoneControlTurnEvent.MutationRequested).decision ==
            PhoneControlTurnDecisionCode.ACCEPTED
    }

    private fun finishPendingTerminalDone() {
        val completed = pendingTerminalDone ?: return
        if (tools.pendingCount > 0) {
            sink.updateTurnPhase(PhoneControlTurnPhase.WORKING)
            return
        }
        pendingTerminalDone = null
        val generation = PhoneControlGenerationId(completed.request.generation)
        val summary = requireNotNull(completed.result.terminalSummary)
        val decision = completeTerminalDone(generation, summary)
        if (decision == PhoneControlTurnDecisionCode.ACCEPTED) {
            deliverToolReceipt(completed)
            return
        }
        outbound.sendPayload(
            buildPhoneControlToolResponse(
                id = completed.request.id,
                name = completed.request.name,
                response = buildJsonObject {
                    put("code", decision.contractValue)
                    put("message", "The turn cannot close until current state is reconciled.")
                    put("effect", "proven_no_effect")
                },
            ),
        )
        sink.updateTurnPhase(PhoneControlTurnPhase.WORKING)
    }

    private fun deliverToolReceipt(completed: PhoneControlCompletedTool) {
        if (!outbound.sendPayload(
            buildPhoneControlToolResponse(
                id = completed.request.id,
                name = completed.request.name,
                response = completed.result.response,
            ),
        )) return
        completed.result.screenFramePayload?.let { payload ->
            if (!outbound.sendEvidence(payload)) return
        }
        if (completed.result.refreshScreenFrame) sink.requestScreenRefresh()
    }

    private fun applyTurnEffects(effects: List<PhoneControlTurnEffect>) {
        val cancelledIds = effects.mapNotNull { effect ->
            (effect as? PhoneControlTurnEffect.CancelJob)?.jobId?.value
        }
        if (cancelledIds.isNotEmpty()) cancelTools(cancelledIds)
        if (effects.any { it is PhoneControlTurnEffect.DiscardGenerationOutput }) {
            sink.discardQueuedPlayback()
        }
    }

    private fun sendRejectedCall(
        call: GeminiLiveFunctionCall,
        generation: PhoneControlGenerationId?,
        code: String,
        message: String,
    ) {
        if (tools.pendingCount > 0) {
            val overflowedBefore = heldToolRejections.overflowed
            heldToolRejections.hold(
                PhoneControlHeldToolRejection(call.id, call.name, generation, code),
            )
            if (!overflowedBefore && heldToolRejections.overflowed) {
                Log.e(TAG, "tool_rejection_overflow generation=${generation?.value ?: -1L}")
            }
            return
        }
        sendRejectedCallNow(call.id, call.name, code, message)
    }

    private fun flushHeldToolRejections() {
        if (tools.pendingCount > 0) return
        if (terminalDone) {
            heldToolRejections.discardHeld()
            return
        }
        for (rejected in heldToolRejections.drainFor(currentGeneration)) {
            if (!sendRejectedCallNow(
                rejected.id,
                rejected.name,
                rejected.code,
                "The tool call was not admitted.",
            )) break
        }
    }

    private fun abortOverflowedSessionIfSettled(): Boolean {
        if (tools.pendingCount > 0 || !heldToolRejections.abandonOverflow()) return false
        pendingTerminalDone = null
        outbound.refuse()
        interruptGeneration()
        return true
    }

    private fun sendRejectedCallNow(
        id: String,
        name: String,
        code: String,
        message: String,
    ): Boolean = outbound.sendPayload(
            buildPhoneControlToolResponse(
                id = id,
                name = name,
                response = buildJsonObject {
                    put("code", code)
                    put("message", message)
                    put("effect", "proven_no_effect")
                },
            ),
        )

    private fun outputChunk(generation: PhoneControlGenerationId): PhoneControlOutputChunk {
        nextOutputSequence = if (nextOutputSequence == Long.MAX_VALUE) 0L else nextOutputSequence + 1L
        return PhoneControlOutputChunk(generation, nextOutputSequence)
    }

    private fun nextOrdinal(current: Long): Long = if (current == Long.MAX_VALUE) 1L else current + 1L

    private companion object {
        const val TAG = "SGTPhoneControlTurn"
        const val DONE_TOOL_NAME = "done"
    }
}
