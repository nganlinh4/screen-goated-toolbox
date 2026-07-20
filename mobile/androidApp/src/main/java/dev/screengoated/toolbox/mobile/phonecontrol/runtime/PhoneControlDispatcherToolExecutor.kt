package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlEffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.tools.PhoneControlToolDispatchBoundary
import dev.screengoated.toolbox.mobile.phonecontrol.tools.PhoneControlToolJobContext
import dev.screengoated.toolbox.mobile.phonecontrol.tools.PhoneControlToolRegistry
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.put
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

/** Adapts the production suspend dispatcher to generation-owned cancellable runtime jobs. */
internal class PhoneControlDispatcherToolExecutor(
    private val boundary: PhoneControlToolDispatchBoundary,
    private val scope: CoroutineScope,
) : PhoneControlToolExecutor {
    override fun execute(
        request: PhoneControlToolRequest,
        completion: PhoneControlToolCompletion,
    ): PhoneControlToolJob {
        val jobContext = PhoneControlToolJobContext(
            turnId = request.turnId,
            jobId = request.id,
            responseGeneration = request.generation,
        )
        val started = AtomicBoolean(false)
        val completed = AtomicBoolean(false)
        val observedCertainty = AtomicReference<PhoneControlEffectCertainty?>(null)
        val cancellationReporterStarted = AtomicBoolean(false)
        val completionLock = Any()
        val mutating = PhoneControlToolRegistry.byName[request.name]?.handler?.mutating == true
        val publishCompletion = { result: PhoneControlToolExecutionResult ->
            val publish = synchronized(completionLock) {
                if (completed.get()) {
                    false
                } else {
                    observedCertainty.set(result.certainty)
                    completed.set(true)
                    true
                }
            }
            if (publish) completion.complete(result)
        }
        val execution = scope.launch {
            started.set(true)
            try {
                val result = withContext(jobContext.effectOwner) {
                    boundary.dispatch(
                        job = jobContext,
                        requestedTool = request.name,
                        arguments = request.arguments as? JsonObject ?: JsonObject(emptyMap()),
                    )
                }
                val certainty = result.response.effectCertainty(mutating)
                publishCompletion(
                    PhoneControlToolExecutionResult(
                        response = result.response,
                        certainty = certainty,
                        terminalSummary = result.terminalSummary,
                        refreshScreenFrame = result.refreshScreenFrame,
                        screenFramePayload = result.screenFramePayload,
                    ),
                )
            } catch (cancelled: CancellationException) {
                withContext(NonCancellable) { jobContext.effectOwner.awaitTerminalEffects() }
                val certainty = jobContext.effectOwner.terminalCertainty(mutating)
                publishCompletion(cancelledExecution(request, certainty))
            } catch (error: Throwable) {
                withContext(NonCancellable) { jobContext.effectOwner.awaitTerminalEffects() }
                val certainty = jobContext.effectOwner.terminalCertainty(mutating)
                publishCompletion(failedExecution(request, certainty, error))
            }
        }
        return DispatcherJob(
            execution = execution,
            started = started,
            completed = completed,
            observedCertainty = observedCertainty,
            effectOwner = jobContext.effectOwner,
            mutating = mutating,
            scope = scope,
            request = request,
            publishCompletion = publishCompletion,
            cancellationReporterStarted = cancellationReporterStarted,
        )
    }

    private class DispatcherJob(
        private val execution: Job,
        private val started: AtomicBoolean,
        private val completed: AtomicBoolean,
        private val observedCertainty: AtomicReference<PhoneControlEffectCertainty?>,
        private val effectOwner: dev.screengoated.toolbox.mobile.phonecontrol.effect.PhoneControlEffectOwner,
        private val mutating: Boolean,
        private val scope: CoroutineScope,
        private val request: PhoneControlToolRequest,
        private val publishCompletion: (PhoneControlToolExecutionResult) -> Unit,
        private val cancellationReporterStarted: AtomicBoolean,
    ) : PhoneControlToolJob {
        override fun cancel(): PhoneControlEffectCertainty {
            if (completed.get()) {
                return observedCertainty.get() ?: PhoneControlEffectCertainty.UNKNOWN
            }
            effectOwner.requestCancellation()
            val certainty = when {
                completed.get() -> observedCertainty.get() ?: PhoneControlEffectCertainty.UNKNOWN
                !started.get() -> PhoneControlEffectCertainty.PROVEN_NO_EFFECT
                else -> effectOwner.terminalCertainty(mutating)
            }
            execution.cancel()
            if (cancellationReporterStarted.compareAndSet(false, true)) {
                scope.launch(start = CoroutineStart.UNDISPATCHED) {
                    withContext(NonCancellable) {
                        execution.join()
                        effectOwner.awaitTerminalEffects()
                        val terminalCertainty = if (started.get()) {
                            effectOwner.terminalCertainty(mutating)
                        } else {
                            PhoneControlEffectCertainty.PROVEN_NO_EFFECT
                        }
                        publishCompletion(cancelledExecution(request, terminalCertainty))
                    }
                }
            }
            return certainty
        }
    }
}

private fun cancelledExecution(
    request: PhoneControlToolRequest,
    certainty: PhoneControlEffectCertainty,
): PhoneControlToolExecutionResult = PhoneControlToolExecutionResult(
    response = kotlinx.serialization.json.buildJsonObject {
        put("code", "tool_cancelled")
        put("message", "The admitted tool job reached its terminal cancellation boundary.")
        put(
            "effect_status",
            when (certainty) {
                PhoneControlEffectCertainty.VERIFIED -> "verified"
                PhoneControlEffectCertainty.MAY_HAVE_OCCURRED -> "may_have_occurred"
                PhoneControlEffectCertainty.PROVEN_NO_EFFECT -> "proven_no_effect"
                PhoneControlEffectCertainty.UNKNOWN -> "unknown"
            },
        )
        put("job_id", request.id)
        put("turn_id", request.turnId)
        put("response_generation", request.generation)
        put("terminal_cancellation_acknowledged", true)
    },
    certainty = certainty,
)

private fun failedExecution(
    request: PhoneControlToolRequest,
    certainty: PhoneControlEffectCertainty,
    error: Throwable,
): PhoneControlToolExecutionResult = PhoneControlToolExecutionResult(
    response = kotlinx.serialization.json.buildJsonObject {
        put("code", "tool_execution_failed")
        put("message", "The admitted tool job failed before producing a normal receipt.")
        put("error_class", error.javaClass.simpleName)
        put(
            "effect_status",
            when (certainty) {
                PhoneControlEffectCertainty.VERIFIED -> "verified"
                PhoneControlEffectCertainty.MAY_HAVE_OCCURRED -> "may_have_occurred"
                PhoneControlEffectCertainty.PROVEN_NO_EFFECT -> "proven_no_effect"
                PhoneControlEffectCertainty.UNKNOWN -> "unknown"
            },
        )
        put("job_id", request.id)
        put("turn_id", request.turnId)
        put("response_generation", request.generation)
    },
    certainty = certainty,
    refreshScreenFrame = certainty != PhoneControlEffectCertainty.PROVEN_NO_EFFECT,
)

private fun JsonObject.effectCertainty(mutating: Boolean): PhoneControlEffectCertainty {
    val wireName = (get("effect_status") as? JsonPrimitive)?.content
    val result = EffectCertainty.entries.firstOrNull { it.wireName == wireName }
        ?: EffectCertainty.UNKNOWN.afterDispatch(mutating)
    return when (result) {
        EffectCertainty.VERIFIED -> PhoneControlEffectCertainty.VERIFIED
        EffectCertainty.MAY_HAVE_OCCURRED -> PhoneControlEffectCertainty.MAY_HAVE_OCCURRED
        EffectCertainty.PROVEN_NO_EFFECT -> PhoneControlEffectCertainty.PROVEN_NO_EFFECT
        EffectCertainty.UNKNOWN -> PhoneControlEffectCertainty.UNKNOWN
    }
}
