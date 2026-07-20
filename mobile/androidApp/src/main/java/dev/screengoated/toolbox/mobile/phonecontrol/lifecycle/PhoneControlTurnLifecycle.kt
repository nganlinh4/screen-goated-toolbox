package dev.screengoated.toolbox.mobile.phonecontrol.lifecycle

internal class PhoneControlTurnLifecycle(
    start: PhoneControlTurnPhase,
    val policy: PhoneControlTurnPolicy = PHONE_CONTROL_TURN_POLICY,
) {
    var phase: PhoneControlTurnPhase = start
        private set

    var activeTurn: PhoneControlTurnId? = null
        private set

    var activeGeneration: PhoneControlGenerationId? = null
        private set

    var finalResponses: Int = 0
        private set

    var finalGenerationCount: Int = 0
        private set

    var reconciliationRequired: Boolean = false
        private set

    var freshObservationRequired: Boolean = false
        private set

    private val jobs = mutableMapOf<PhoneControlJobId, PhoneControlTurnJob>()
    private val observedTargets = mutableMapOf<PhoneControlTargetId, PhoneControlTargetIdentity>()

    private var activeGenerationState: PhoneControlGenerationState? = null
    private var retiredGenerationThrough: Long? = null
    private var currentSnapshotGeneration: PhoneControlSnapshotGeneration? = null
    private var turnClosed = start == PhoneControlTurnPhase.IDLE

    init {
        require(policy.maximumFinalResponsesPerUserTurn == 1)
        require(policy.maximumAdmittedToolJobs == 1)
        require(policy.maximumHeldToolRejections > 0)
        require(policy.rejectionOverflowAbandonsSession)
        require(policy.doneIsTerminal)
        require(!policy.cleanupProducesOutput)
        require(!policy.currentGenerationAudioBlockedByTools)
        require(policy.lateRetiredEventsAreAbsorbed)
        require(policy.unknownMutationRequiresReconciliation)
        require(policy.completionRequiresNoPendingJobs)
        require(policy.reconciliationBlocksMutationAndCompletion)
    }

    fun reduce(event: PhoneControlTurnEvent): PhoneControlTurnTransition {
        val transition = when (event) {
            is PhoneControlTurnEvent.JobReceipt -> onJobReceipt(event)
            is PhoneControlTurnEvent.TerminalDone -> onTerminalDone(event)
            is PhoneControlTurnEvent.JobRequested -> onJobRequested(event)
            is PhoneControlTurnEvent.AudioReceived -> onAudioReceived(event)
            is PhoneControlTurnEvent.CaptionReceived -> onCaptionReceived(event)
            is PhoneControlTurnEvent.CleanupCompleted -> PhoneControlTurnTransition(
                decision = PhoneControlTurnDecisionCode.ACCEPTED,
                effects = listOf(PhoneControlTurnEffect.CleanupAcknowledged(event.generation)),
            )
            is PhoneControlTurnEvent.AssistantContentReceived -> onAssistantContent(event)
            is PhoneControlTurnEvent.GenerationCompleted -> onGenerationCompleted(event)
            is PhoneControlTurnEvent.UserBargeIn -> onUserBargeIn(event)
            is PhoneControlTurnEvent.GenerationInterrupted -> onGenerationInterrupted(event)
            is PhoneControlTurnEvent.MutationInterrupted -> onMutationInterrupted(event)
            PhoneControlTurnEvent.MutationRequested -> onMutationRequested()
            is PhoneControlTurnEvent.FreshObservation -> onFreshObservation(event)
            is PhoneControlTurnEvent.TargetObserved -> onTargetObserved(event)
            is PhoneControlTurnEvent.SurfaceChanged -> onSurfaceChanged(event)
            is PhoneControlTurnEvent.TargetActionRequested -> onTargetActionRequested(event)
            is PhoneControlTurnEvent.TransportFailed -> onTransportFailed(event)
            is PhoneControlTurnEvent.SocketOpened -> onSocketOpened(event)
        }
        check(finalResponses <= policy.maximumFinalResponsesPerUserTurn)
        return transition
    }

    fun job(id: PhoneControlJobId): PhoneControlTurnJob? = jobs[id]

    fun generation(id: PhoneControlGenerationId): PhoneControlGenerationState? {
        activeGenerationState?.takeIf { it.id == id }?.let { return it }
        return if (isGenerationRetired(id)) {
            PhoneControlGenerationState(id, PhoneControlGenerationStatus.RETIRED)
        } else {
            null
        }
    }

    fun isGenerationRetired(id: PhoneControlGenerationId): Boolean {
        return retiredGenerationThrough?.let { boundary -> id.value <= boundary } == true
    }

    fun retainedStateCounts(): PhoneControlRetainedStateCounts = PhoneControlRetainedStateCounts(
        activeGenerationStates = if (activeGenerationState == null) 0 else 1,
        retiredGenerationBoundaries = if (retiredGenerationThrough == null) 0 else 1,
        jobRecords = jobs.size,
        cancelledJobRecords = jobs.values.count { it.status == PhoneControlJobStatus.CANCELLED },
        observedTargetRecords = observedTargets.size,
    )

    val turnRemainsActive: Boolean
        get() = !turnClosed && phase != PhoneControlTurnPhase.IDLE

    private fun onJobReceipt(
        event: PhoneControlTurnEvent.JobReceipt,
    ): PhoneControlTurnTransition {
        val generation = event.generation
            ?: return PhoneControlTurnTransition(PhoneControlTurnDecisionCode.ABSORBED)
        val jobId = event.jobId
            ?: return PhoneControlTurnTransition(PhoneControlTurnDecisionCode.ABSORBED)
        val job = jobs[jobId]
            ?: return PhoneControlTurnTransition(PhoneControlTurnDecisionCode.ABSORBED)
        if (job.ownerGeneration != generation) {
            return PhoneControlTurnTransition(PhoneControlTurnDecisionCode.ABSORBED)
        }
        if (job.status == PhoneControlJobStatus.COMPLETED) {
            return PhoneControlTurnTransition(PhoneControlTurnDecisionCode.ABSORBED)
        }

        val effects = mutableListOf<PhoneControlTurnEffect>()
        if (job.status == PhoneControlJobStatus.CANCELLED) {
            jobs.remove(jobId)
            appendReconciliationEffect(event, effects)
            return PhoneControlTurnTransition(
                decision = PhoneControlTurnDecisionCode.ABSORBED,
                effects = effects,
            )
        }

        val ownership = generationDecision(generation)
        if (ownership != PhoneControlTurnDecisionCode.ACCEPTED) {
            return PhoneControlTurnTransition(ownership)
        }
        jobs[jobId] = job.copy(status = PhoneControlJobStatus.COMPLETED)
        appendReconciliationEffect(event, effects)
        effects += PhoneControlTurnEffect.DeliverJobReceipt(
            generation = generation,
            jobId = jobId,
            certainty = event.certainty,
        )
        return PhoneControlTurnTransition(
            decision = PhoneControlTurnDecisionCode.ACCEPTED,
            effects = effects,
        )
    }

    private fun onTerminalDone(
        event: PhoneControlTurnEvent.TerminalDone,
    ): PhoneControlTurnTransition {
        if (reconciliationRequired) {
            return PhoneControlTurnTransition(
                PhoneControlTurnDecisionCode.BLOCKED_RECONCILIATION_REQUIRED,
            )
        }
        if (turnClosed || phase == PhoneControlTurnPhase.FINALIZING) {
            return PhoneControlTurnTransition(PhoneControlTurnDecisionCode.TURN_CLOSED)
        }
        val generation = event.generation ?: activeGeneration
        val ownership = generation?.let(::generationDecision)
            ?: PhoneControlTurnDecisionCode.ACCEPTED
        if (ownership != PhoneControlTurnDecisionCode.ACCEPTED) {
            return PhoneControlTurnTransition(ownership)
        }
        if (generation != null && hasUnsettledJob(generation)) {
            return PhoneControlTurnTransition(PhoneControlTurnDecisionCode.TOOL_CALL_IN_FLIGHT)
        }

        val contentSeen = event.assistantContentSeen
            ?: generation?.let(::generation)?.assistantContentSeen
            ?: true
        val effects = mutableListOf<PhoneControlTurnEffect>()
        generation?.let { effects += retireGeneration(it) }
        if (!contentSeen && finalGenerationCount == 0) {
            finalGenerationCount = 1
            phase = PhoneControlTurnPhase.FINALIZING
            effects += PhoneControlTurnEffect.FinalGenerationRequested(generation)
        } else {
            effects += finishTurn()
        }
        return PhoneControlTurnTransition(
            decision = PhoneControlTurnDecisionCode.ACCEPTED,
            effects = effects,
        )
    }

    private fun onJobRequested(
        event: PhoneControlTurnEvent.JobRequested,
    ): PhoneControlTurnTransition {
        if (phase == PhoneControlTurnPhase.FINALIZING) {
            return PhoneControlTurnTransition(PhoneControlTurnDecisionCode.TURN_CLOSED)
        }
        val ownership = generationDecision(event.generation)
        if (ownership != PhoneControlTurnDecisionCode.ACCEPTED) {
            return PhoneControlTurnTransition(ownership)
        }
        if (event.jobId != null && jobs.containsKey(event.jobId)) {
            return PhoneControlTurnTransition(PhoneControlTurnDecisionCode.DUPLICATE_EVENT)
        }
        if (jobs.values.count { it.status != PhoneControlJobStatus.COMPLETED } >=
            policy.maximumAdmittedToolJobs
        ) {
            return PhoneControlTurnTransition(PhoneControlTurnDecisionCode.TOOL_CALL_IN_FLIGHT)
        }
        bindGeneration(event.generation)
        event.jobId?.let { jobId ->
            jobs[jobId] = PhoneControlTurnJob(
                id = jobId,
                ownerTurn = activeTurn,
                ownerGeneration = event.generation,
            )
        }
        return PhoneControlTurnTransition(
            decision = PhoneControlTurnDecisionCode.ACCEPTED,
            effects = listOf(
                PhoneControlTurnEffect.DispatchJob(event.generation, event.jobId),
            ),
        )
    }

    private fun onAudioReceived(
        event: PhoneControlTurnEvent.AudioReceived,
    ): PhoneControlTurnTransition {
        val ownership = generationDecision(event.chunk.generation)
        if (ownership != PhoneControlTurnDecisionCode.ACCEPTED) {
            return PhoneControlTurnTransition(ownership)
        }
        bindGeneration(event.chunk.generation)
        return PhoneControlTurnTransition(
            decision = PhoneControlTurnDecisionCode.ACCEPTED,
            effects = listOf(PhoneControlTurnEffect.PlayAudio(event.chunk)),
        )
    }

    private fun onCaptionReceived(
        event: PhoneControlTurnEvent.CaptionReceived,
    ): PhoneControlTurnTransition {
        val ownership = generationDecision(event.chunk.generation)
        if (ownership != PhoneControlTurnDecisionCode.ACCEPTED) {
            return PhoneControlTurnTransition(ownership)
        }
        bindGeneration(event.chunk.generation)
        return PhoneControlTurnTransition(
            decision = PhoneControlTurnDecisionCode.ACCEPTED,
            effects = listOf(PhoneControlTurnEffect.DeliverCaption(event.chunk)),
        )
    }

    private fun onAssistantContent(
        event: PhoneControlTurnEvent.AssistantContentReceived,
    ): PhoneControlTurnTransition {
        val ownership = generationDecision(event.generation)
        if (ownership != PhoneControlTurnDecisionCode.ACCEPTED) {
            return PhoneControlTurnTransition(ownership)
        }
        bindGeneration(event.generation, assistantContentSeen = true)
        return PhoneControlTurnTransition(PhoneControlTurnDecisionCode.ACCEPTED)
    }

    private fun onGenerationCompleted(
        event: PhoneControlTurnEvent.GenerationCompleted,
    ): PhoneControlTurnTransition {
        val ownership = generationDecision(event.generation)
        if (ownership != PhoneControlTurnDecisionCode.ACCEPTED) {
            return PhoneControlTurnTransition(ownership)
        }
        if (hasUnsettledJob(event.generation)) {
            return PhoneControlTurnTransition(PhoneControlTurnDecisionCode.TOOL_CALL_IN_FLIGHT)
        }
        bindGeneration(event.generation)
        val effects = mutableListOf<PhoneControlTurnEffect>()
        effects += retireGeneration(event.generation)
        effects += finishTurn()
        return PhoneControlTurnTransition(
            decision = PhoneControlTurnDecisionCode.ACCEPTED,
            effects = effects,
        )
    }

    private fun onUserBargeIn(
        event: PhoneControlTurnEvent.UserBargeIn,
    ): PhoneControlTurnTransition {
        val effects = mutableListOf<PhoneControlTurnEffect>()
        val retired = activeGeneration
        if (retired != null) {
            effects += retireGeneration(retired)
            effects += PhoneControlTurnEffect.DiscardGenerationOutput(retired)
            jobs.entries.forEach { entry ->
                if (
                    entry.value.ownerGeneration == retired &&
                    entry.value.status == PhoneControlJobStatus.PENDING
                ) {
                    entry.setValue(entry.value.copy(status = PhoneControlJobStatus.CANCELLED))
                    effects += PhoneControlTurnEffect.CancelJob(entry.key)
                }
            }
        }

        activeTurn = event.newTurn
        phase = PhoneControlTurnPhase.WORKING
        turnClosed = false
        finalResponses = 0
        finalGenerationCount = 0
        bindGeneration(event.newGeneration)
        effects += PhoneControlTurnEffect.ActivateTurn(event.newTurn, event.newGeneration)
        return PhoneControlTurnTransition(
            decision = PhoneControlTurnDecisionCode.ACCEPTED,
            effects = effects,
        )
    }

    private fun onGenerationInterrupted(
        event: PhoneControlTurnEvent.GenerationInterrupted,
    ): PhoneControlTurnTransition {
        val ownership = generationDecision(event.generation)
        if (ownership != PhoneControlTurnDecisionCode.ACCEPTED) {
            return PhoneControlTurnTransition(ownership)
        }
        val effects = mutableListOf<PhoneControlTurnEffect>()
        effects += retireGeneration(event.generation)
        effects += PhoneControlTurnEffect.DiscardGenerationOutput(event.generation)
        jobs.entries.forEach { entry ->
            if (
                entry.value.ownerGeneration == event.generation &&
                entry.value.status == PhoneControlJobStatus.PENDING
            ) {
                entry.setValue(entry.value.copy(status = PhoneControlJobStatus.CANCELLED))
                effects += PhoneControlTurnEffect.CancelJob(entry.key)
            }
        }
        phase = PhoneControlTurnPhase.LISTENING
        activeTurn = null
        turnClosed = true
        return PhoneControlTurnTransition(
            decision = PhoneControlTurnDecisionCode.ACCEPTED,
            effects = effects,
        )
    }

    private fun onMutationInterrupted(
        event: PhoneControlTurnEvent.MutationInterrupted,
    ): PhoneControlTurnTransition {
        if (!event.certainty.requiresReconciliation) {
            return PhoneControlTurnTransition(PhoneControlTurnDecisionCode.ACCEPTED)
        }
        reconciliationRequired = policy.unknownMutationRequiresReconciliation
        return PhoneControlTurnTransition(
            decision = PhoneControlTurnDecisionCode.ACCEPTED,
            effects = listOf(
                PhoneControlTurnEffect.ReconciliationRequired(
                    generation = activeGeneration,
                    jobId = null,
                ),
            ),
        )
    }

    private fun onMutationRequested(): PhoneControlTurnTransition {
        if (reconciliationRequired) {
            return PhoneControlTurnTransition(
                PhoneControlTurnDecisionCode.BLOCKED_RECONCILIATION_REQUIRED,
            )
        }
        return PhoneControlTurnTransition(
            decision = PhoneControlTurnDecisionCode.ACCEPTED,
            effects = listOf(PhoneControlTurnEffect.AuthorizeMutation),
        )
    }

    private fun onFreshObservation(
        event: PhoneControlTurnEvent.FreshObservation,
    ): PhoneControlTurnTransition {
        val effects = mutableListOf<PhoneControlTurnEffect>()
        if (event.stateReconciled) {
            reconciliationRequired = false
            freshObservationRequired = false
            effects += PhoneControlTurnEffect.ReconciliationCleared
        }
        return PhoneControlTurnTransition(
            decision = PhoneControlTurnDecisionCode.ACCEPTED,
            effects = effects,
        )
    }

    private fun onTargetObserved(
        event: PhoneControlTurnEvent.TargetObserved,
    ): PhoneControlTurnTransition {
        advanceSnapshot(event.target.snapshotGeneration)
        observedTargets[event.target.id] = event.target
        freshObservationRequired = false
        return PhoneControlTurnTransition(PhoneControlTurnDecisionCode.ACCEPTED)
    }

    private fun onSurfaceChanged(
        event: PhoneControlTurnEvent.SurfaceChanged,
    ): PhoneControlTurnTransition {
        advanceSnapshot(event.snapshotGeneration)
        return PhoneControlTurnTransition(PhoneControlTurnDecisionCode.ACCEPTED)
    }

    private fun onTargetActionRequested(
        event: PhoneControlTurnEvent.TargetActionRequested,
    ): PhoneControlTurnTransition {
        val observed = observedTargets[event.target.id]
        val stale = observed != event.target ||
            currentSnapshotGeneration != event.target.snapshotGeneration
        if (stale) {
            freshObservationRequired = true
            return PhoneControlTurnTransition(
                decision = PhoneControlTurnDecisionCode.STALE_TARGET,
                effects = listOf(PhoneControlTurnEffect.FreshObservationRequired),
            )
        }
        return PhoneControlTurnTransition(
            decision = PhoneControlTurnDecisionCode.ACCEPTED,
            effects = listOf(PhoneControlTurnEffect.PerformTargetAction(event.target)),
        )
    }

    private fun onTransportFailed(
        event: PhoneControlTurnEvent.TransportFailed,
    ): PhoneControlTurnTransition {
        val ownership = generationDecision(event.generation)
        if (ownership != PhoneControlTurnDecisionCode.ACCEPTED) {
            return PhoneControlTurnTransition(ownership)
        }
        bindGeneration(event.generation)
        return PhoneControlTurnTransition(
            decision = PhoneControlTurnDecisionCode.ACCEPTED,
            effects = listOf(PhoneControlTurnEffect.RecoverTransport(event.generation)),
        )
    }

    private fun onSocketOpened(
        event: PhoneControlTurnEvent.SocketOpened,
    ): PhoneControlTurnTransition {
        val ownership = generationDecision(event.generation)
        if (ownership != PhoneControlTurnDecisionCode.ACCEPTED) {
            return PhoneControlTurnTransition(ownership)
        }
        bindGeneration(event.generation)
        return PhoneControlTurnTransition(
            decision = PhoneControlTurnDecisionCode.ACCEPTED,
            effects = listOf(PhoneControlTurnEffect.AcceptSocket(event.generation)),
        )
    }

    private fun generationDecision(
        generation: PhoneControlGenerationId,
    ): PhoneControlTurnDecisionCode {
        if (isGenerationRetired(generation)) return PhoneControlTurnDecisionCode.ABSORBED
        if (turnClosed) return PhoneControlTurnDecisionCode.TURN_CLOSED
        val active = activeGeneration
        return if (active == null || active == generation) {
            PhoneControlTurnDecisionCode.ACCEPTED
        } else {
            PhoneControlTurnDecisionCode.WRONG_GENERATION
        }
    }

    private fun bindGeneration(
        generation: PhoneControlGenerationId,
        assistantContentSeen: Boolean = false,
    ) {
        activeGeneration = generation
        val existing = activeGenerationState?.takeIf { it.id == generation }
        activeGenerationState = PhoneControlGenerationState(
            id = generation,
            status = PhoneControlGenerationStatus.ACTIVE,
            assistantContentSeen = assistantContentSeen || existing?.assistantContentSeen == true,
        )
    }

    private fun retireGeneration(
        generation: PhoneControlGenerationId,
    ): PhoneControlTurnEffect.RetireGeneration {
        retiredGenerationThrough = maxOf(retiredGenerationThrough ?: generation.value, generation.value)
        jobs.entries.removeAll { (_, job) ->
            job.ownerGeneration == generation && job.status == PhoneControlJobStatus.COMPLETED
        }
        if (activeGeneration == generation) {
            activeGeneration = null
            activeGenerationState = null
        }
        return PhoneControlTurnEffect.RetireGeneration(generation)
    }

    private fun hasUnsettledJob(generation: PhoneControlGenerationId): Boolean {
        return jobs.values.any { job ->
            job.ownerGeneration == generation && job.status != PhoneControlJobStatus.COMPLETED
        }
    }

    private fun appendReconciliationEffect(
        event: PhoneControlTurnEvent.JobReceipt,
        effects: MutableList<PhoneControlTurnEffect>,
    ) {
        if (!event.certainty.requiresReconciliation) return
        reconciliationRequired = policy.unknownMutationRequiresReconciliation
        effects += PhoneControlTurnEffect.ReconciliationRequired(
            generation = event.generation,
            jobId = event.jobId,
        )
    }

    private fun advanceSnapshot(generation: PhoneControlSnapshotGeneration) {
        if (currentSnapshotGeneration != generation) observedTargets.clear()
        currentSnapshotGeneration = generation
    }

    private fun finishTurn(): PhoneControlTurnEffect.FinalResponseReady {
        if (finalResponses < policy.maximumFinalResponsesPerUserTurn) {
            finalResponses += 1
        }
        phase = PhoneControlTurnPhase.IDLE
        activeGeneration = null
        activeGenerationState = null
        turnClosed = true
        return PhoneControlTurnEffect.FinalResponseReady(activeTurn)
    }
}
