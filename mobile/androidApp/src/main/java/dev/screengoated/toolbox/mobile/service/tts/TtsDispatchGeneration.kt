package dev.screengoated.toolbox.mobile.service.tts

import java.util.concurrent.atomic.AtomicLong

/** Serializes nonblocking dispatch with global interruption. */
internal class TtsDispatchGeneration {
    private val generation = AtomicLong(0)

    fun current(): Long = generation.get()

    @Synchronized fun advance(): Long = generation.incrementAndGet()

    @Synchronized fun runIfCurrent(expected: Long, action: () -> Boolean): Boolean =
        expected == generation.get() && action()
}

internal fun TtsRequest.withDispatchGeneration(gate: TtsDispatchGeneration, generation: Long): TtsRequest = copy(
    isCurrent = { generation == gate.current() && canDispatch() },
    runIfCurrent = { effect ->
        var dispatched = false
        dispatchIfCurrent { dispatched = gate.runIfCurrent(generation) { effect(); true } } && dispatched
    },
)

/** Called only after admission succeeds; rejects leave the previous preview current. */
internal class SettingsPreviewAdmission {
    private val generation = AtomicLong()

    fun admit(request: TtsRequest): TtsRequest {
        val owner = generation.incrementAndGet()
        return request.copy(
            isCurrent = { owner == generation.get() && request.canDispatch() },
            runIfCurrent = { effect ->
                var admitted = false
                request.dispatchIfCurrent {
                    if (owner == generation.get()) {
                        admitted = true
                        effect()
                    }
                } && admitted
            },
        )
    }
}
