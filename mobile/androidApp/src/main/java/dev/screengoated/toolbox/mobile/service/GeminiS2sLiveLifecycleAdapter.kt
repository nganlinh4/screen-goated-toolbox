package dev.screengoated.toolbox.mobile.service

import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveClassifiedError
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveConnectedSession
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleEffect
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleEvent
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleFrame
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecyclePolicy
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleState
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveReadySession
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSessionException
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSessionFailure
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSessionLifecycle
import kotlinx.coroutines.CancellationException

internal data class GeminiS2sLiveConnection(
    val generation: Long,
    val session: GeminiLiveReadySession,
)

private data class GeminiS2sPendingConnection(
    val generation: Long,
    val session: GeminiLiveConnectedSession,
)

/**
 * Thin executor for the shared lifecycle reducer.
 *
 * Socket open and setup activation execute the reducer's distinct [OpenSocket] and [SendSetup]
 * effects. A ready session is published only after the structural setup acknowledgement.
 */
internal class GeminiS2sLiveLifecycleAdapter(
    private val clockMs: () -> Long,
    private val openConnectedSession: suspend () -> GeminiLiveConnectedSession,
    private val setupPayload: () -> String,
    private val onEffect: (GeminiLiveLifecycleEffect) -> Unit = {},
) {
    private val policy = GeminiLiveLifecyclePolicy.continuous()
    private val lifecycle = GeminiLiveSessionLifecycle(policy = policy)
    private var pendingConnection: GeminiS2sPendingConnection? = null
    private var connection: GeminiS2sLiveConnection? = null

    val state: GeminiLiveLifecycleState
        get() = lifecycle.state

    val activeConnection: GeminiS2sLiveConnection?
        get() = connection

    suspend fun ensureReady(): GeminiS2sLiveConnection? {
        val event = when (state.phase) {
            dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecyclePhase.IDLE -> {
                GeminiLiveLifecycleEvent.Start
            }
            else -> GeminiLiveLifecycleEvent.Tick
        }
        applyEffects(lifecycle.reduce(clockMs(), event))
        return connection
    }

    fun inputSent(chunks: Long = 1) {
        lifecycle.reduce(clockMs(), GeminiLiveLifecycleEvent.InputSent(chunks))
    }

    fun inputActivity() {
        lifecycle.reduce(clockMs(), GeminiLiveLifecycleEvent.InputActivity)
    }

    fun updateWorkState(
        pendingWorkCount: Long,
        bufferedInputCount: Long,
        userSpeaking: Boolean,
    ) {
        lifecycle.reduce(
            clockMs(),
            GeminiLiveLifecycleEvent.WorkState(
                pendingWorkCount = pendingWorkCount,
                bufferedInputCount = bufferedInputCount,
                userSpeaking = userSpeaking,
            ),
        )
    }

    suspend fun observeFrame(
        frame: GeminiLiveLifecycleFrame,
    ): List<GeminiLiveLifecycleEffect> {
        return applyEffects(lifecycle.reduce(clockMs(), GeminiLiveLifecycleEvent.Frame(frame)))
    }

    suspend fun transportFailed(generation: Long): List<GeminiLiveLifecycleEffect> {
        return applyEffects(
            lifecycle.reduce(
                clockMs(),
                GeminiLiveLifecycleEvent.TransportFailure(
                    generation = generation,
                    retryable = true,
                ),
            ),
        )
    }

    suspend fun serverError(
        generation: Long,
        retryable: Boolean,
        kind: String = "server",
    ): List<GeminiLiveLifecycleEffect> {
        return observeFrame(
            GeminiLiveLifecycleFrame(
                generation = generation,
                error = GeminiLiveClassifiedError(kind = kind, retryable = retryable),
            ),
        )
    }

    suspend fun tick(): List<GeminiLiveLifecycleEffect> {
        return applyEffects(lifecycle.reduce(clockMs(), GeminiLiveLifecycleEvent.Tick))
    }

    suspend fun cancel() {
        applyEffects(lifecycle.reduce(clockMs(), GeminiLiveLifecycleEvent.Cancel))
    }

    private suspend fun applyEffects(
        effects: List<GeminiLiveLifecycleEffect>,
    ): List<GeminiLiveLifecycleEffect> {
        val featureEffects = mutableListOf<GeminiLiveLifecycleEffect>()
        for (effect in effects) {
            onEffect(effect)
            when (effect) {
                is GeminiLiveLifecycleEffect.OpenSocket -> open(effect.generation)
                is GeminiLiveLifecycleEffect.SendSetup -> activate(effect.generation)
                is GeminiLiveLifecycleEffect.CloseSocket -> close(effect.generation)
                is GeminiLiveLifecycleEffect.ScheduleReconnect,
                is GeminiLiveLifecycleEffect.ReportFailure,
                GeminiLiveLifecycleEffect.CancelSession,
                -> Unit
                else -> featureEffects += effect
            }
        }
        return featureEffects
    }

    private suspend fun open(generation: Long) {
        val opened = try {
            openConnectedSession()
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: GeminiLiveSessionException) {
            val server = error.failure as? GeminiLiveSessionFailure.Server
            if (server != null) {
                serverError(generation, retryable = server.retryable)
                if (!server.retryable) throw error
                return
            }
            transportFailed(generation)
            return
        } catch (_: Throwable) {
            transportFailed(generation)
            return
        }

        val setupEffects = lifecycle.reduce(
            clockMs(),
            GeminiLiveLifecycleEvent.SocketOpened(generation),
        )
        check(setupEffects == listOf(GeminiLiveLifecycleEffect.SendSetup(generation))) {
            opened.close()
            "connected Gemini Live session did not enter setup lifecycle"
        }
        pendingConnection = GeminiS2sPendingConnection(generation, opened)
        applyEffects(setupEffects)
    }

    private suspend fun activate(generation: Long) {
        val pending = pendingConnection
        if (pending == null || pending.generation != generation) return
        val ready = try {
            pending.session.activate(setupPayload(), policy.setupTimeoutMs)
        } catch (cancelled: CancellationException) {
            pending.session.close()
            if (pendingConnection === pending) pendingConnection = null
            throw cancelled
        } catch (error: GeminiLiveSessionException) {
            when (error.failure) {
                GeminiLiveSessionFailure.SetupTimedOut -> {
                    val timeoutAt = maxOf(
                        clockMs(),
                        requireNotNull(state.setupDeadlineMs) {
                            "setup timeout arrived without a reducer deadline"
                        },
                    )
                    applyEffects(lifecycle.reduce(timeoutAt, GeminiLiveLifecycleEvent.Tick))
                }
                is GeminiLiveSessionFailure.Server -> {
                    serverError(generation, retryable = error.failure.retryable)
                    if (!error.failure.retryable) throw error
                }
                else -> transportFailed(generation)
            }
            return
        } catch (_: Throwable) {
            transportFailed(generation)
            return
        }

        if (
            state.generation != generation ||
            state.phase != dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecyclePhase.AWAITING_SETUP
        ) {
            ready.close()
            if (pendingConnection === pending) pendingConnection = null
            return
        }
        pendingConnection = null
        connection = GeminiS2sLiveConnection(generation, ready)
        applyEffects(
            lifecycle.reduce(
                clockMs(),
                GeminiLiveLifecycleEvent.Frame(
                    GeminiLiveLifecycleFrame(generation = generation, setupComplete = true),
                ),
            ),
        )
    }

    private fun close(generation: Long) {
        val pending = pendingConnection
        if (pending?.generation == generation) {
            pending.session.close()
            pendingConnection = null
        }
        val current = connection ?: return
        if (current.generation != generation) return
        current.session.close()
        connection = null
    }
}

internal class LiveTranslatePendingAudio(
    private val maxSamples: Int,
    private val frameSamples: Int,
) {
    private val chunks = ArrayDeque<ShortArray>()

    var sampleCount: Int = 0
        private set

    init {
        require(maxSamples >= frameSamples)
        require(frameSamples > 0)
    }

    fun append(chunk: ShortArray) {
        if (chunk.isEmpty()) return
        chunks.addLast(chunk.copyOf())
        sampleCount += chunk.size
        dropOldest((sampleCount - maxSamples).coerceAtLeast(0))
    }

    fun takeFirst(): ShortArray? {
        if (sampleCount < frameSamples) return null
        val frame = ShortArray(frameSamples)
        var outputOffset = 0
        while (outputOffset < frameSamples) {
            val first = chunks.removeFirst()
            val copied = minOf(first.size, frameSamples - outputOffset)
            first.copyInto(frame, destinationOffset = outputOffset, endIndex = copied)
            outputOffset += copied
            if (copied < first.size) {
                chunks.addFirst(first.copyOfRange(copied, first.size))
            }
        }
        sampleCount -= frameSamples
        return frame
    }

    fun restoreFirst(frame: ShortArray) {
        if (frame.isEmpty()) return
        require(frame.size == frameSamples)
        chunks.addFirst(frame.copyOf())
        sampleCount += frame.size
        dropNewest((sampleCount - maxSamples).coerceAtLeast(0))
    }

    private fun dropOldest(count: Int) {
        var remaining = count
        while (remaining > 0) {
            val first = chunks.removeFirst()
            val removed = minOf(remaining, first.size)
            remaining -= removed
            sampleCount -= removed
            if (removed < first.size) {
                chunks.addFirst(first.copyOfRange(removed, first.size))
            }
        }
    }

    private fun dropNewest(count: Int) {
        var remaining = count
        while (remaining > 0) {
            val last = chunks.removeLast()
            val removed = minOf(remaining, last.size)
            remaining -= removed
            sampleCount -= removed
            if (removed < last.size) {
                chunks.addLast(last.copyOfRange(0, last.size - removed))
            }
        }
    }
}
