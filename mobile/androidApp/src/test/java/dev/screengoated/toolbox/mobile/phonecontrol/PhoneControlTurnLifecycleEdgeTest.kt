package dev.screengoated.toolbox.mobile.phonecontrol

import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlEffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlGenerationId
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlJobId
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlJobStatus
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlOutputChunk
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlSnapshotGeneration
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTargetId
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTargetIdentity
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnDecisionCode
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnEffect
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnEvent
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnId
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnLifecycle
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnPhase
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnTransition
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PhoneControlTurnLifecycleEdgeTest {
    @Test
    fun `disconnect during an action requires explicit uncertainty before completion blocks`() {
        val lifecycle = workingLifecycle()
        val generation = PhoneControlGenerationId(1)
        val job = PhoneControlJobId("mutation-1")

        val requested = lifecycle.reduce(
            PhoneControlTurnEvent.JobRequested(generation, job),
        )
        val disconnected = lifecycle.reduce(
            PhoneControlTurnEvent.TransportFailed(generation),
        )

        assertTrue(requested.hasEffect<PhoneControlTurnEffect.DispatchJob>())
        assertTrue(disconnected.hasEffect<PhoneControlTurnEffect.RecoverTransport>())
        assertFalse(lifecycle.reconciliationRequired)

        val interrupted = lifecycle.reduce(
            PhoneControlTurnEvent.MutationInterrupted(
                PhoneControlEffectCertainty.MAY_HAVE_OCCURRED,
            ),
        )
        val done = lifecycle.reduce(PhoneControlTurnEvent.TerminalDone(generation))

        assertTrue(interrupted.hasEffect<PhoneControlTurnEffect.ReconciliationRequired>())
        assertTrue(lifecycle.reconciliationRequired)
        assertEquals(
            PhoneControlTurnDecisionCode.BLOCKED_RECONCILIATION_REQUIRED,
            done.decision,
        )
        assertFalse(done.hasEffect<PhoneControlTurnEffect.FinalResponseReady>())
    }

    @Test
    fun `barge in cancels pending owner and prunes completed job records`() {
        val lifecycle = workingLifecycle()
        val oldGeneration = PhoneControlGenerationId(11)
        val pending = PhoneControlJobId("pending")
        val completed = PhoneControlJobId("completed")
        lifecycle.reduce(PhoneControlTurnEvent.JobRequested(oldGeneration, completed))
        lifecycle.reduce(
            PhoneControlTurnEvent.JobReceipt(
                generation = oldGeneration,
                jobId = completed,
                certainty = PhoneControlEffectCertainty.VERIFIED,
            ),
        )
        lifecycle.reduce(PhoneControlTurnEvent.JobRequested(oldGeneration, pending))

        val barge = lifecycle.reduce(
            PhoneControlTurnEvent.UserBargeIn(
                newTurn = PhoneControlTurnId(12),
                newGeneration = PhoneControlGenerationId(12),
            ),
        )

        assertEquals(PhoneControlJobStatus.CANCELLED, lifecycle.job(pending)?.status)
        assertEquals(null, lifecycle.job(completed))
        assertEquals(listOf(pending), barge.effects.filterIsInstance<PhoneControlTurnEffect.CancelJob>()
            .map(PhoneControlTurnEffect.CancelJob::jobId))
        assertTrue(lifecycle.isGenerationRetired(oldGeneration))
        assertEquals(PhoneControlGenerationId(12), lifecycle.activeGeneration)
        assertTrue(lifecycle.turnRemainsActive)
    }

    @Test
    fun `unknown and completed receipts cannot deliver or require reconciliation`() {
        val lifecycle = workingLifecycle()
        val generation = PhoneControlGenerationId(14)
        val job = PhoneControlJobId("known")

        val unknown = lifecycle.reduce(
            PhoneControlTurnEvent.JobReceipt(
                generation = generation,
                jobId = PhoneControlJobId("unknown"),
                certainty = PhoneControlEffectCertainty.MAY_HAVE_OCCURRED,
            ),
        )
        lifecycle.reduce(PhoneControlTurnEvent.JobRequested(generation, job))
        lifecycle.reduce(
            PhoneControlTurnEvent.JobReceipt(
                generation = generation,
                jobId = job,
                certainty = PhoneControlEffectCertainty.VERIFIED,
            ),
        )
        val duplicate = lifecycle.reduce(
            PhoneControlTurnEvent.JobReceipt(
                generation = generation,
                jobId = job,
                certainty = PhoneControlEffectCertainty.MAY_HAVE_OCCURRED,
            ),
        )

        assertEquals(PhoneControlTurnDecisionCode.ABSORBED, unknown.decision)
        assertEquals(PhoneControlTurnDecisionCode.ABSORBED, duplicate.decision)
        assertFalse(unknown.hasEffect<PhoneControlTurnEffect.DeliverJobReceipt>())
        assertFalse(duplicate.hasEffect<PhoneControlTurnEffect.DeliverJobReceipt>())
        assertFalse(unknown.hasEffect<PhoneControlTurnEffect.ReconciliationRequired>())
        assertFalse(duplicate.hasEffect<PhoneControlTurnEffect.ReconciliationRequired>())
        assertFalse(lifecycle.reconciliationRequired)
        assertEquals(PhoneControlJobStatus.COMPLETED, lifecycle.job(job)?.status)
    }

    @Test
    fun `cancelled owner settles once and releases its job id`() {
        val lifecycle = workingLifecycle()
        val generation = PhoneControlGenerationId(15)
        val job = PhoneControlJobId("reusable")
        lifecycle.reduce(PhoneControlTurnEvent.JobRequested(generation, job))
        lifecycle.reduce(
            PhoneControlTurnEvent.UserBargeIn(
                newTurn = PhoneControlTurnId(16),
                newGeneration = PhoneControlGenerationId(16),
            ),
        )

        val blocked = lifecycle.reduce(
            PhoneControlTurnEvent.JobRequested(PhoneControlGenerationId(16), PhoneControlJobId("later")),
        )
        val settled = lifecycle.reduce(
            PhoneControlTurnEvent.JobReceipt(
                generation = generation,
                jobId = job,
                certainty = PhoneControlEffectCertainty.MAY_HAVE_OCCURRED,
            ),
        )
        val reused = lifecycle.reduce(
            PhoneControlTurnEvent.JobRequested(PhoneControlGenerationId(16), job),
        )

        assertEquals(PhoneControlTurnDecisionCode.TOOL_CALL_IN_FLIGHT, blocked.decision)
        assertEquals(PhoneControlTurnDecisionCode.ABSORBED, settled.decision)
        assertFalse(settled.hasEffect<PhoneControlTurnEffect.DeliverJobReceipt>())
        assertTrue(settled.hasEffect<PhoneControlTurnEffect.ReconciliationRequired>())
        assertTrue(lifecycle.reconciliationRequired)
        assertTrue(reused.hasEffect<PhoneControlTurnEffect.DispatchJob>())
        assertEquals(0, lifecycle.retainedStateCounts().cancelledJobRecords)
    }

    @Test
    fun `completed id can be reused after retirement and old receipt cannot settle new owner`() {
        val lifecycle = workingLifecycle()
        val job = PhoneControlJobId("server-id")
        val oldGeneration = PhoneControlGenerationId(17)
        lifecycle.reduce(PhoneControlTurnEvent.JobRequested(oldGeneration, job))
        lifecycle.reduce(
            PhoneControlTurnEvent.JobReceipt(
                oldGeneration,
                job,
                PhoneControlEffectCertainty.VERIFIED,
            ),
        )
        lifecycle.reduce(PhoneControlTurnEvent.GenerationCompleted(oldGeneration))
        lifecycle.reduce(
            PhoneControlTurnEvent.UserBargeIn(
                PhoneControlTurnId(18),
                PhoneControlGenerationId(18),
            ),
        )

        val reused = lifecycle.reduce(
            PhoneControlTurnEvent.JobRequested(PhoneControlGenerationId(18), job),
        )
        val lateOldReceipt = lifecycle.reduce(
            PhoneControlTurnEvent.JobReceipt(
                oldGeneration,
                job,
                PhoneControlEffectCertainty.MAY_HAVE_OCCURRED,
            ),
        )

        assertTrue(reused.hasEffect<PhoneControlTurnEffect.DispatchJob>())
        assertEquals(PhoneControlTurnDecisionCode.ABSORBED, lateOldReceipt.decision)
        assertFalse(lateOldReceipt.hasEffect<PhoneControlTurnEffect.DeliverJobReceipt>())
        assertFalse(lateOldReceipt.hasEffect<PhoneControlTurnEffect.ReconciliationRequired>())
        assertEquals(PhoneControlJobStatus.PENDING, lifecycle.job(job)?.status)
        assertFalse(lifecycle.reconciliationRequired)
    }

    @Test
    fun `interruption retires work without manufacturing a replacement turn`() {
        val lifecycle = workingLifecycle()
        val generation = PhoneControlGenerationId(13)
        val pending = PhoneControlJobId("pending-interrupted")
        lifecycle.reduce(PhoneControlTurnEvent.JobRequested(generation, pending))

        val interrupted = lifecycle.reduce(
            PhoneControlTurnEvent.GenerationInterrupted(generation),
        )

        assertEquals(PhoneControlTurnDecisionCode.ACCEPTED, interrupted.decision)
        assertEquals(PhoneControlJobStatus.CANCELLED, lifecycle.job(pending)?.status)
        assertTrue(lifecycle.isGenerationRetired(generation))
        assertEquals(null, lifecycle.activeTurn)
        assertEquals(null, lifecycle.activeGeneration)
        assertEquals(PhoneControlTurnPhase.LISTENING, lifecycle.phase)
        assertFalse(lifecycle.turnRemainsActive)
        assertTrue(interrupted.hasEffect<PhoneControlTurnEffect.DiscardGenerationOutput>())
        assertTrue(interrupted.hasEffect<PhoneControlTurnEffect.CancelJob>())
    }

    @Test
    fun `duplicate terminal completion cannot emit a second final response`() {
        val lifecycle = workingLifecycle()
        val generation = PhoneControlGenerationId(21)
        lifecycle.reduce(PhoneControlTurnEvent.AssistantContentReceived(generation))

        val first = lifecycle.reduce(PhoneControlTurnEvent.TerminalDone(generation))
        val duplicate = lifecycle.reduce(PhoneControlTurnEvent.TerminalDone(generation))

        assertEquals(PhoneControlTurnDecisionCode.ACCEPTED, first.decision)
        assertTrue(first.hasEffect<PhoneControlTurnEffect.FinalResponseReady>())
        assertFalse(duplicate.hasEffect<PhoneControlTurnEffect.FinalResponseReady>())
        assertEquals(1, lifecycle.finalResponses)
        assertEquals(PhoneControlTurnPhase.IDLE, lifecycle.phase)
    }

    @Test
    fun `uncertain cancelled result is absorbed reconciled and released`() {
        val lifecycle = workingLifecycle()
        val oldGeneration = PhoneControlGenerationId(31)
        val oldJob = PhoneControlJobId("old-job")
        lifecycle.reduce(PhoneControlTurnEvent.JobRequested(oldGeneration, oldJob))
        lifecycle.reduce(
            PhoneControlTurnEvent.UserBargeIn(
                newTurn = PhoneControlTurnId(32),
                newGeneration = PhoneControlGenerationId(32),
            ),
        )

        val receipt = lifecycle.reduce(
            PhoneControlTurnEvent.JobReceipt(
                generation = oldGeneration,
                jobId = oldJob,
                certainty = PhoneControlEffectCertainty.MAY_HAVE_OCCURRED,
            ),
        )

        assertEquals(PhoneControlTurnDecisionCode.ABSORBED, receipt.decision)
        assertFalse(receipt.hasEffect<PhoneControlTurnEffect.DeliverJobReceipt>())
        assertTrue(receipt.hasEffect<PhoneControlTurnEffect.ReconciliationRequired>())
        assertTrue(lifecycle.reconciliationRequired)
        assertEquals(null, lifecycle.job(oldJob))
    }

    @Test
    fun `stale target cannot dispatch and a fresh identity can`() {
        val lifecycle = workingLifecycle()
        val oldTarget = target("action", 41)
        lifecycle.reduce(PhoneControlTurnEvent.TargetObserved(oldTarget))
        lifecycle.reduce(
            PhoneControlTurnEvent.SurfaceChanged(PhoneControlSnapshotGeneration(42)),
        )
        assertEquals(0, lifecycle.retainedStateCounts().observedTargetRecords)

        val stale = lifecycle.reduce(PhoneControlTurnEvent.TargetActionRequested(oldTarget))

        assertEquals(PhoneControlTurnDecisionCode.STALE_TARGET, stale.decision)
        assertTrue(stale.hasEffect<PhoneControlTurnEffect.FreshObservationRequired>())
        assertFalse(stale.hasEffect<PhoneControlTurnEffect.PerformTargetAction>())
        assertTrue(lifecycle.freshObservationRequired)

        val freshTarget = target("action", 42)
        lifecycle.reduce(PhoneControlTurnEvent.TargetObserved(freshTarget))
        val accepted = lifecycle.reduce(
            PhoneControlTurnEvent.TargetActionRequested(freshTarget),
        )

        assertEquals(PhoneControlTurnDecisionCode.ACCEPTED, accepted.decision)
        assertTrue(accepted.hasEffect<PhoneControlTurnEffect.PerformTargetAction>())
        assertFalse(lifecycle.freshObservationRequired)
    }

    @Test
    fun `current generation output is gated by ownership without retained local dedupe`() {
        val lifecycle = workingLifecycle()
        val generation = PhoneControlGenerationId(43)
        val chunk = PhoneControlOutputChunk(generation, 1)

        val firstAudio = lifecycle.reduce(PhoneControlTurnEvent.AudioReceived(chunk))
        val repeatedAudio = lifecycle.reduce(PhoneControlTurnEvent.AudioReceived(chunk))
        val firstCaption = lifecycle.reduce(PhoneControlTurnEvent.CaptionReceived(chunk))
        val repeatedCaption = lifecycle.reduce(PhoneControlTurnEvent.CaptionReceived(chunk))

        assertTrue(firstAudio.hasEffect<PhoneControlTurnEffect.PlayAudio>())
        assertTrue(repeatedAudio.hasEffect<PhoneControlTurnEffect.PlayAudio>())
        assertTrue(firstCaption.hasEffect<PhoneControlTurnEffect.DeliverCaption>())
        assertTrue(repeatedCaption.hasEffect<PhoneControlTurnEffect.DeliverCaption>())
    }

    @Test
    fun `thousands of turns retain only live generation job and snapshot authority`() {
        val lifecycle = workingLifecycle()

        repeat(5_000) { index ->
            val ordinal = index.toLong() + 100
            val generation = PhoneControlGenerationId(ordinal)
            val job = PhoneControlJobId("job-$ordinal")
            lifecycle.reduce(
                PhoneControlTurnEvent.UserBargeIn(PhoneControlTurnId(ordinal), generation),
            )
            lifecycle.reduce(PhoneControlTurnEvent.JobRequested(generation, job))
            lifecycle.reduce(
                PhoneControlTurnEvent.AudioReceived(PhoneControlOutputChunk(generation, ordinal)),
            )
            lifecycle.reduce(
                PhoneControlTurnEvent.CaptionReceived(PhoneControlOutputChunk(generation, ordinal)),
            )
            lifecycle.reduce(
                PhoneControlTurnEvent.TargetObserved(target("target-$ordinal", ordinal)),
            )
            lifecycle.reduce(
                PhoneControlTurnEvent.JobReceipt(
                    generation,
                    job,
                    PhoneControlEffectCertainty.VERIFIED,
                ),
            )
            lifecycle.reduce(PhoneControlTurnEvent.GenerationCompleted(generation))
        }

        val retained = lifecycle.retainedStateCounts()
        assertEquals(0, retained.activeGenerationStates)
        assertEquals(1, retained.retiredGenerationBoundaries)
        assertEquals(0, retained.jobRecords)
        assertEquals(0, retained.cancelledJobRecords)
        assertEquals(1, retained.observedTargetRecords)
        assertTrue(lifecycle.isGenerationRetired(PhoneControlGenerationId(100)))
        assertTrue(lifecycle.isGenerationRetired(PhoneControlGenerationId(5_099)))
    }

    private fun workingLifecycle(): PhoneControlTurnLifecycle {
        return PhoneControlTurnLifecycle(PhoneControlTurnPhase.WORKING)
    }

    private fun target(id: String, generation: Long): PhoneControlTargetIdentity {
        return PhoneControlTargetIdentity(
            PhoneControlTargetId(id),
            PhoneControlSnapshotGeneration(generation),
        )
    }

    private inline fun <reified T : PhoneControlTurnEffect>
        PhoneControlTurnTransition.hasEffect(): Boolean {
        return effects.any { effect -> effect is T }
    }
}
