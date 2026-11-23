package dev.screengoated.toolbox.mobile.phonecontrol.runtime
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlEffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlGenerationId
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlJobId
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnEffect
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnEvent
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnId
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnLifecycle
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnDecisionCode
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnPhase
import dev.screengoated.toolbox.mobile.phonecontrol.tools.PhoneControlToolRegistry
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveFunctionCall
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleEffect
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveServerFrame
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.Channel
internal class PhoneControlTurnCoordinator(
    executor: PhoneControlToolExecutor,
    scope: CoroutineScope,
    private val sink: PhoneControlTurnSink,
    private val recorder: PhoneControlTurnRecorder = NoOpPhoneControlTurnRecorder,
    private val cleanup: PhoneControlTurnCleanup = NoOpPhoneControlTurnCleanup,
) {
    private val completionEvents = Channel<PhoneControlToolCompletionEvent>(
        capacity = PHONE_CONTROL_COMPLETION_QUEUE_CAPACITY,
    )
    private val tools = PhoneControlToolController(executor, scope, completionEvents)
    private val lifecycle = PhoneControlTurnLifecycle(PhoneControlTurnPhase.LISTENING)
    private val inputTranscript = PhoneControlInputTranscriptAssembler()
    private val inputEpochGate = PhoneControlInputEpochGate()
    private val assistantTranscript = PhoneControlTranscriptAccumulator()
    private val heldToolRejections = PhoneControlHeldToolRejections()
    private val outbound = PhoneControlTurnOutbound(sink)
    private var nextTurn = 0L
    private var nextGeneration = 0L
    private val outputSequencer = PhoneControlOutputSequencer()
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
        if (inputEpochGate.takePendingEpoch()) inputTranscript.beginEpoch()
        val interrupted = effects.any {
            it == GeminiLiveLifecycleEffect.FinalizeInterruptedGeneration
        }
        if (interrupted) {
            sink.interruptPlayback()
            interruptGeneration()
            inputTranscript.beginEpoch()
            sink.requestScreenRefresh()
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
                is GeminiLiveLifecycleEffect.CancelTools -> cancelTools(effect.ids, providerInput = true)
                GeminiLiveLifecycleEffect.FinalizeGeneration,
                GeminiLiveLifecycleEffect.FinalizeTurn,
                is GeminiLiveLifecycleEffect.FinalizeResponse,
                -> if (!interrupted) completeGeneration(frameGeneration)
                else -> Unit
            }
        }
    }
    fun userSpeechActivity(assistantPlaybackActive: Boolean, burstStarted: Boolean): Boolean =
        inputEpochGate.observe(assistantPlaybackActive, burstStarted)
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
                        outbound.sendRejectedTool(
                            completed.request.id,
                            completed.request.name,
                            "duplicate_terminal",
                            "A terminal completion is already pending.",
                        )
                    }
                } else {
                    outbound.deliverToolReceipt(completed)
                }
            } else {
                Log.d(TAG, "absorbed_tool_receipt id=${completed.request.id} decision=${transition.decision.contractValue}")
            }
        }
        if (abortOverflowedSessionIfSettled()) return
        flushHeldToolRejections()
        finishPendingTerminalDone()
    }
    fun abandonProtocolSession() {
        outbound.block()
        retireLocalGeneration()
        tools.cancelAll()
        heldToolRejections.reset()
        pendingTerminalDone = null
    }
    fun retireForProtectedCheckpoint(): Boolean {
        if (tools.pendingCount > 0) return false
        retireLocalGeneration()
        return true
    }
    fun retireTransportInterruptedTurn(): Boolean {
        val generation = currentGeneration
        if (!lifecycle.turnRemainsActive || generation == null) return false
        if (tools.pendingCount > 0) {
            // An accepted effect is still settling, so its generation survives the reconnect.
            val decision = lifecycle.reduce(PhoneControlTurnEvent.TransportFailed(generation)).decision
            Log.i(TAG, "transport_turn_kept generation=${generation.value} decision=${decision.contractValue}")
            return false
        }
        retireLocalGeneration()
        return true
    }
    /** A ready connection carries only a generation the reducer still owns; a retired one is never revived. */
    fun transportReady() {
        val generation = currentGeneration ?: return
        val transition = lifecycle.reduce(PhoneControlTurnEvent.SocketOpened(generation))
        if (transition.effects.any { it is PhoneControlTurnEffect.AcceptSocket }) return
        Log.w(TAG, "transport_generation_absorbed generation=${generation.value} decision=${transition.decision.contractValue}")
        retireLocalGeneration()
    }
    fun freshProtocolSessionBound() {
        outbound.reset()
        if (!lifecycle.turnRemainsActive) inputTranscript.beginEpoch()
    }
    /** A typed goal has no input transcript: its send opens the turn, and a user-authored goal is its request. */
    fun userInterfaceGoalSent(userRequest: String? = null) {
        if (currentGeneration == null) activateNewTurn()
        userRequest?.let { text -> lifecycle.activeTurn?.let { recorder.userRequestSubmitted(it.value, text) } }
    }
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
            sink.updateConversationInputCaption(update.text)
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
        inputEpochGate.collecting()
        // A newly admitted user turn owns playback immediately. Stop and retire
        // any audio that was still draining from the preceding turn before the
        // new generation can enqueue output.
        sink.interruptPlayback()
        sink.discardQueuedPlayback()
        heldToolRejections.discardHeld()
        val replacedTurn = lifecycle.activeTurn?.takeIf { lifecycle.turnRemainsActive }
        val replacedGeneration = currentGeneration
        nextTurn = outputSequencer.nextOrdinal(nextTurn)
        nextGeneration = outputSequencer.nextOrdinal(nextGeneration)
        val turn = PhoneControlTurnId(nextTurn)
        val generation = PhoneControlGenerationId(nextGeneration)
        inputTranscript.claimCurrentEpoch()
        assistantTranscript.reset()
        val transition = lifecycle.reduce(PhoneControlTurnEvent.UserBargeIn(turn, generation))
        applyTurnEffects(transition.effects)
        replacedTurn?.let {
            recorder.turnInterrupted(it.value)
            retireTurnCleanup(it, replacedGeneration)
        }
        currentGeneration = generation
        finalGeneration = null
        terminalDone = false
        terminalSummary = null
        pendingTerminalDone = null
        generationAwaitingReconciliation = null
        recorder.turnStarted(turn.value, generation.value)
        sink.updateConversationOrb(orbThinkingPresentation.stateLabel, null)
        sink.updateConversationOutputCaption("")
        sink.updateConversationPhase(PhoneControlTurnPhase.WORKING)
        return generation
    }

    private fun interruptGeneration() {
        inputEpochGate.collecting()
        val generation = currentGeneration ?: return
        val turn = lifecycle.activeTurn
        val transition = lifecycle.reduce(
            PhoneControlTurnEvent.GenerationInterrupted(generation),
        )
        applyTurnEffects(transition.effects)
        if (transition.decision == PhoneControlTurnDecisionCode.ACCEPTED) {
            turn?.let {
                recorder.turnInterrupted(it.value)
                retireTurnCleanup(it, generation)
            }
        }
        currentGeneration = null
        finalGeneration = null
        terminalDone = false
        terminalSummary = null
        pendingTerminalDone = null
        generationAwaitingReconciliation = null
        heldToolRejections.discardHeld()
        sink.updateConversationPhase(PhoneControlTurnPhase.LISTENING)
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
            frame.audioParts.isNotEmpty()
        if (!assistantContent || generation == null) return
        if (!sink.surfaceAssistantContent()) return
        sink.updateConversationOrb(orbRespondingPresentation.stateLabel, null)

        val contentTransition = lifecycle.reduce(
            PhoneControlTurnEvent.AssistantContentReceived(generation),
        )
        if (contentTransition.decision != PhoneControlTurnDecisionCode.ACCEPTED) return
        sink.updateConversationPhase(lifecycle.phase)

        val caption = frame.outputTranscript?.takeIf(String::isNotBlank)
        caption?.let { text ->
            val chunk = outputSequencer.chunk(generation)
            val transition = lifecycle.reduce(PhoneControlTurnEvent.CaptionReceived(chunk))
            if (transition.effects.any { it is PhoneControlTurnEffect.DeliverCaption }) {
                if (assistantTranscript.merge(text)) {
                    sink.updateConversationOutputCaption(assistantTranscript.text)
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
            val chunk = outputSequencer.chunk(generation)
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
        val turnId = lifecycle.activeTurn?.value ?: 0L
        Log.i(TAG, call.structuralDispatchLog(generation.value, turnId))
        call.orbPresentation().let {
            sink.updateConversationOrb(it.stateLabel, it.iconOverride)
        }
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
        sink.updateConversationPhase(PhoneControlTurnPhase.WORKING)
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
        sink.updateConversationOrb(orbDonePresentation.stateLabel, null)
        if (transition.effects.any { it is PhoneControlTurnEffect.FinalGenerationRequested }) {
            nextGeneration = outputSequencer.nextOrdinal(nextGeneration)
            finalGeneration = PhoneControlGenerationId(nextGeneration)
            currentGeneration = finalGeneration
            sink.updateConversationPhase(PhoneControlTurnPhase.FINALIZING)
        } else {
            currentGeneration = null
            recordCompletedTurn(turn)
            sink.updateConversationPhase(PhoneControlTurnPhase.IDLE)
        }
        return transition.decision
    }

    private fun completeGeneration(generation: PhoneControlGenerationId?) {
        if (generation == null) return
        if (tools.pendingCount > 0 || pendingTerminalDone != null) {
            sink.updateConversationPhase(PhoneControlTurnPhase.WORKING)
            return
        }
        if (lifecycle.reconciliationRequired) {
            generationAwaitingReconciliation = generation
            sink.updateConversationPhase(PhoneControlTurnPhase.WORKING)
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
            sink.updateConversationPhase(PhoneControlTurnPhase.IDLE)
        }
    }

    private fun recordCompletedTurn(turn: PhoneControlTurnId?) {
        turn ?: return
        inputEpochGate.completed(turn.value)
        val assistant = assistantTranscript.text.ifBlank { terminalSummary.orEmpty() }
        recorder.turnCompleted(turn.value, inputTranscript.text, assistant)
        retireTurnCleanup(turn, generation = null)
    }

    /** Silent local cleanup of a retired turn; the reducer guarantees cleanup produces no output. */
    private fun retireTurnCleanup(turn: PhoneControlTurnId, generation: PhoneControlGenerationId?) {
        cleanup.retire(turn.value)
        val transition = lifecycle.reduce(PhoneControlTurnEvent.CleanupCompleted(generation))
        check(transition.effects.all { it is PhoneControlTurnEffect.CleanupAcknowledged })
    }

    private fun retireLocalGeneration() {
        sink.interruptPlayback()
        sink.discardQueuedPlayback()
        interruptGeneration()
    }

    private fun cancelTools(ids: List<String>, providerInput: Boolean = false) {
        if (providerInput && tools.activeRequest?.id in ids) {
            inputEpochGate.collecting()
            inputTranscript.beginEpoch()
            sink.interruptPlayback()
            sink.discardQueuedPlayback()
            sink.requestScreenRefresh()
        }
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
            sink.updateConversationPhase(PhoneControlTurnPhase.WORKING)
            return
        }
        pendingTerminalDone = null
        val generation = PhoneControlGenerationId(completed.request.generation)
        val summary = requireNotNull(completed.result.terminalSummary)
        val decision = completeTerminalDone(generation, summary)
        if (decision == PhoneControlTurnDecisionCode.ACCEPTED) {
            outbound.deliverToolReceipt(completed)
            return
        }
        outbound.sendRejectedTool(
            completed.request.id,
            completed.request.name,
            decision.contractValue,
            "The turn cannot close until current state is reconciled.",
        )
        sink.updateConversationPhase(PhoneControlTurnPhase.WORKING)
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
        outbound.sendRejectedTool(call.id, call.name, code, message)
    }

    private fun flushHeldToolRejections() {
        if (tools.pendingCount > 0) return
        if (terminalDone) {
            heldToolRejections.discardHeld()
            return
        }
        for (rejected in heldToolRejections.drainFor(currentGeneration)) {
            if (!outbound.sendRejectedTool(
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

    private companion object {
        const val TAG = "SGTPhoneControlTurn"
        const val DONE_TOOL_NAME = "done"
    }
}
