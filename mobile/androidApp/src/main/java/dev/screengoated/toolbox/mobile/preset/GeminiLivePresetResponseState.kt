package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleEffect
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleEvent
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleFrame
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecyclePhase
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecyclePolicy
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSessionLifecycle

internal typealias GeminiLivePresetResponsePolicy = GeminiLiveLifecyclePolicy

internal enum class GeminiLivePresetWaitOutcome { PENDING, COMPLETE, FIRST_OUTPUT_TIMEOUT, HARD_TIMEOUT, CLOSED, FAILED }

internal class GeminiLivePresetResponseState(
    policy: GeminiLivePresetResponsePolicy,
    private val requireInteractionIdle: Boolean,
    startedMillis: Long,
) {
    private val lifecycle = GeminiLiveSessionLifecycle(policy.copy(
        contentIdleMs = if (requireInteractionIdle) null else policy.contentIdleMs,
        hardResponseTimeoutMs = if (requireInteractionIdle) policy.hardResponseTimeoutMs else null,
    ))
    private var terminal: GeminiLivePresetWaitOutcome? = null
    val contentStarted: Boolean get() = lifecycle.state.hasOutput

    init {
        // The accepted transport enters its finite response epoch after input admission.
        lifecycle.reduce(startedMillis, GeminiLiveLifecycleEvent.Start)
        val generation = lifecycle.state.generation
        lifecycle.reduce(startedMillis, GeminiLiveLifecycleEvent.SocketOpened(generation))
        lifecycle.reduce(startedMillis, GeminiLiveLifecycleEvent.Frame(GeminiLiveLifecycleFrame(
            generation = generation, setupComplete = true,
        )))
    }

    fun observeText(nowMillis: Long, text: String) {
        if (text.isNotEmpty()) lifecycle.reduce(nowMillis, GeminiLiveLifecycleEvent.Frame(
            GeminiLiveLifecycleFrame(generation = lifecycle.state.generation, contentCount = 1),
        ))
    }

    fun outcome(nowMillis: Long, complete: Boolean = false, closed: Boolean = false): GeminiLivePresetWaitOutcome {
        terminal?.let { return it }
        if (closed && !(contentStarted && !requireInteractionIdle)) {
            terminal = GeminiLivePresetWaitOutcome.CLOSED
            return GeminiLivePresetWaitOutcome.CLOSED
        }
        if (complete || closed) lifecycle.reduce(nowMillis, GeminiLiveLifecycleEvent.Frame(
            GeminiLiveLifecycleFrame(generation = lifecycle.state.generation, turnComplete = true),
        ))
        val effects = lifecycle.reduce(nowMillis, GeminiLiveLifecycleEvent.Tick)
        val result = if (lifecycle.state.phase == GeminiLiveLifecyclePhase.COMPLETED) GeminiLivePresetWaitOutcome.COMPLETE
        else effects.filterIsInstance<GeminiLiveLifecycleEffect.ReportFailure>().firstOrNull()?.let {
            when (it.reason) {
                "firstResponseTimeout" -> GeminiLivePresetWaitOutcome.FIRST_OUTPUT_TIMEOUT
                "hardResponseTimeout" -> GeminiLivePresetWaitOutcome.HARD_TIMEOUT
                else -> GeminiLivePresetWaitOutcome.FAILED
            }
        } ?: if (lifecycle.state.phase == GeminiLiveLifecyclePhase.FAILED) GeminiLivePresetWaitOutcome.FAILED
        else GeminiLivePresetWaitOutcome.PENDING
        if (result != GeminiLivePresetWaitOutcome.PENDING) terminal = result
        return result
    }

    fun remainingMillis(nowMillis: Long): Long = listOfNotNull(
        lifecycle.state.firstResponseDeadlineMs,
        lifecycle.state.contentIdleDeadlineMs,
        lifecycle.state.hardResponseDeadlineMs,
    ).minOrNull()?.let { (it - nowMillis).coerceAtLeast(0) } ?: Long.MAX_VALUE
}
