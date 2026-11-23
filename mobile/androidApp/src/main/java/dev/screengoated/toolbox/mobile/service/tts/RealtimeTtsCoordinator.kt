package dev.screengoated.toolbox.mobile.service.tts

import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.RealtimeTtsSettings
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.Executors

/** Serializes producer state without holding a monitor during runtime operations. */
class RealtimeTtsCoordinator(private val runtime: TtsRuntimeService) {
    private val dispatcher = Executors.newSingleThreadExecutor { runnable ->
        Thread(runnable, "sgt-realtime-tts-owner").apply { isDaemon = true }
    }.asCoroutineDispatcher()
    private val scope = CoroutineScope(SupervisorJob() + dispatcher)
    private val commands = Channel<Command>(Channel.UNLIMITED)
    private val closed = AtomicBoolean(false)
    private val generation = TtsDispatchGeneration()
    private val actorJob: kotlinx.coroutines.Job
    private val playbackJob: kotlinx.coroutines.Job
    private var spokenLength = 0
    private var queuedLength = 0
    private var initialized = false
    private var pending: Pending? = null
    private var retryEnd: Int? = null
    private var producerFence: TtsInterruption? = null
    private var admissionRetryToken = 0L
    private var admissionRetryScheduled = false
    private var admissionRetryDelayMs = 1_000L
    private val waitingEnds = ArrayDeque<Int>()
    private var latest: Input? = null
    private var lastText: String? = null
    private var lastRevision: Long? = null

    private data class Pending(val requestId: Long, val end: Int)
    private data class Input(
        val text: String,
        val language: String,
        val global: MobileGlobalTtsSettings,
        val realtime: RealtimeTtsSettings,
    )
    private sealed interface Command {
        data class Action(val run: () -> Unit, val done: CompletableDeferred<Unit>) : Command
        data class Playback(val event: TtsPlaybackEvent, val done: CompletableDeferred<Unit>) : Command
        data class RetryAdmission(val token: Long) : Command
    }

    init {
        val actor = scope.launch {
            for (command in commands) {
                when (command) {
                    is Command.Action -> try {
                        command.run()
                        command.done.complete(Unit)
                    } catch (error: Throwable) {
                        command.done.completeExceptionally(error)
                    }
                    is Command.Playback -> {
                        try {
                            if (!closed.get()) handlePlayback(command.event)
                        } catch (error: Exception) {
                            android.util.Log.e("RealtimeTts", "Could not advance realtime playback", error)
                        } finally { command.done.complete(Unit) }
                    }
                    is Command.RetryAdmission -> if (!closed.get() && command.token == admissionRetryToken) {
                        admissionRetryScheduled = false
                        try { enqueueLatest() } catch (error: Exception) {
                            android.util.Log.e("RealtimeTts", "Could not retry realtime playback", error)
                        }
                    }
                }
            }
        }
        actorJob = actor
        playbackJob = scope.launch(start = CoroutineStart.UNDISPATCHED) {
            runtime.playbackEventsFor(
                TtsEventSelector(TtsConsumer.REALTIME, OWNER_TOKEN),
            ).collect { event ->
                val done = CompletableDeferred<Unit>()
                if (event.consumer == TtsConsumer.REALTIME && event.ownerToken == OWNER_TOKEN) {
                    if (commands.trySend(Command.Playback(event, done)).isSuccess) done.await()
                }
            }
        }
    }

    fun update(
        committedText: String,
        targetLanguage: String,
        globalSettings: MobileGlobalTtsSettings,
        realtimeSettings: RealtimeTtsSettings,
        translationVisible: Boolean,
        translationRevision: Long = 0L,
    ) = execute {
        when {
            !translationVisible -> stopInternal(reset = false)
            !realtimeSettings.enabled -> stopInternal(reset = true)
            else -> updateInternal(committedText, targetLanguage, globalSettings, realtimeSettings, translationRevision)
        }
    }

    fun stop() = execute { stopInternal(reset = false) }
    fun stopAndReset() = execute { stopInternal(reset = true) }

    /** Dispose only with the owning service, not on an ordinary session restart. */
    fun close() {
        if (!closed.compareAndSet(false, true)) return
        playbackJob.cancel()
        val done = CompletableDeferred<Unit>()
        if (commands.trySend(Command.Action({
            retireProducer()
            latest = null
            runtime.setRealtimeQueueDepth(0)
        }, done)).isSuccess) runBlocking { done.await() }
        commands.close()
        runBlocking { actorJob.join() }
        scope.cancel()
        dispatcher.close()
    }

    private fun execute(action: () -> Unit) {
        if (closed.get()) return
        val done = CompletableDeferred<Unit>()
        if (!commands.trySend(Command.Action({ if (!closed.get()) action() }, done)).isSuccess) return
        runBlocking { done.await() }
    }

    private fun updateInternal(
        committedText: String,
        targetLanguage: String,
        globalSettings: MobileGlobalTtsSettings,
        realtimeSettings: RealtimeTtsSettings,
        translationRevision: Long,
    ) {
        val normalized = committedText.trimEnd()
        if (lastRevision != null && lastRevision != translationRevision) {
            retireProducer()
            resetOffsets()
            latest = null
            lastText = null
        }
        lastRevision = translationRevision
        if (normalized.isBlank()) {
            retireProducer()
            resetOffsets()
            latest = null
            lastText = null
            runtime.setRealtimeQueueDepth(0)
            return
        }
        if (lastText?.let { !normalized.startsWith(it) } == true) {
            retireProducer()
            resetOffsets()
        }
        if (normalized.length < spokenLength || normalized.length < queuedLength) {
            retireProducer()
            resetOffsets()
        }
        if (retryEnd?.let { normalized.length < it } == true) {
            retireProducer()
            resetOffsets()
        }
        val currentFence = runtime.realtimeContinuationFence()
        if (producerFence != null && producerFence != currentFence) {
            val retiredLength = latest?.text?.length?.coerceAtMost(normalized.length) ?: 0
            retireProducer()
            spokenLength = retiredLength
            queuedLength = retiredLength
            initialized = true
        }
        producerFence = currentFence
        latest = Input(normalized, targetLanguage, globalSettings, realtimeSettings)
        lastText = normalized
        while (waitingEnds.isNotEmpty() && waitingEnds.last() > normalized.length) waitingEnds.removeLast()
        val furthest = waitingEnds.lastOrNull() ?: retryEnd ?: queuedLength
        if (pending != null || retryEnd != null || waitingEnds.isNotEmpty()) {
            if (normalized.length > furthest) {
                if (waitingEnds.size == MAX_WAITING_SEGMENTS) waitingEnds.removeLast()
                waitingEnds.addLast(normalized.length)
            }
        }
        enqueueLatest()
    }

    private fun enqueueLatest() {
        val input = latest ?: return
        val normalized = input.text
        if (pending != null) {
            updateDepth()
            return
        }
        if (!initialized) {
            initialized = true
            if (normalized.codePointCount(0, normalized.length) > 50) {
                val boundary = normalized.dropLast(1).lastIndexOfAny(charArrayOf('.', '?', '!', '\n'))
                if (boundary >= 0) {
                    spokenLength = boundary + 1
                    queuedLength = spokenLength
                }
            }
        }
        val nextEnd = retryEnd ?: waitingEnds.firstOrNull() ?: normalized.length
        if (nextEnd <= queuedLength) {
            updateDepth()
            return
        }
        val nextSegment = normalized.substring(queuedLength, nextEnd)
        if (nextSegment.isBlank()) {
            if (retryEnd != null) retryEnd = null else if (waitingEnds.isNotEmpty()) waitingEnds.removeFirst()
            queuedLength = nextEnd
            if (queuedLength < normalized.length) enqueueLatest() else updateDepth()
            return
        }
        val current = generation.current()
        val requestId = runtime.enqueueRealtime(
            TtsRequest(
                text = nextSegment,
                consumer = TtsConsumer.REALTIME,
                priority = TtsPriority.REALTIME,
                requestMode = TtsRequestMode.REALTIME,
                settingsSnapshot = input.global.toRuntimeSnapshot(input.language, input.realtime),
                ownerToken = OWNER_TOKEN,
                isCurrent = { generation.current() == current },
                runIfCurrent = { action -> generation.runIfCurrent(current) { action(); true } },
                resumeAfterInterruption = producerFence,
            ),
        )
        if (requestId == TTS_REQUEST_REJECTED) {
            scheduleAdmissionRetry()
            updateDepth()
            return
        }
        if (requestId == TTS_REQUEST_CANCELLED) {
            invalidateAdmissionRetry()
            updateDepth()
            return
        }
        invalidateAdmissionRetry()
        if (retryEnd != null) { retryEnd = null } else if (waitingEnds.isNotEmpty()) waitingEnds.removeFirst()
        queuedLength = nextEnd
        pending = Pending(requestId, nextEnd)
        updateDepth()
    }

    private fun handlePlayback(event: TtsPlaybackEvent) {
        val current = pending?.takeIf { it.requestId == event.requestId } ?: return
        pending = null
        when (event.completionStatus) {
            TtsCompletionStatus.COMPLETED, TtsCompletionStatus.PARTIAL -> {
                spokenLength = maxOf(spokenLength, current.end)
                queuedLength = spokenLength
                updateDepth()
                enqueueLatest()
            }
            TtsCompletionStatus.INTERRUPTED, TtsCompletionStatus.FAILED -> {
                queuedLength = spokenLength
                retryEnd = current.end
                runtime.setRealtimeQueueDepth(0)
                if (event.completionStatus == TtsCompletionStatus.INTERRUPTED && event.interruptedBy?.replacementRequestId != null) enqueueLatest()
            }
        }
    }

    private fun stopInternal(reset: Boolean) {
        if (!reset) pending?.let { spokenLength = maxOf(spokenLength, it.end) }
        retireProducer()
        latest = null
        if (reset) {
            resetOffsets()
            lastText = null
            lastRevision = null
        } else queuedLength = spokenLength
        runtime.setRealtimeQueueDepth(0)
    }

    private fun retireProducer() {
        pending?.let {
            generation.advance()
            runtime.retireRealtime(it.requestId, OWNER_TOKEN)
        }
        pending = null
        retryEnd = null
        producerFence = null
        waitingEnds.clear()
        invalidateAdmissionRetry()
    }

    private fun resetOffsets() {
        pending = null
        retryEnd = null
        waitingEnds.clear()
        spokenLength = 0
        queuedLength = 0
        initialized = false
    }

    private fun updateDepth() {
        runtime.setRealtimeQueueDepth((if (pending != null) 1 else 0) +
            (if (retryEnd != null) 1 else 0) + waitingEnds.size)
    }

    private fun scheduleAdmissionRetry() {
        if (admissionRetryScheduled || latest == null) return
        admissionRetryScheduled = true
        val token = ++admissionRetryToken
        val delayMs = admissionRetryDelayMs
        admissionRetryDelayMs = (delayMs * 2).coerceAtMost(30_000L)
        scope.launch {
            delay(delayMs)
            commands.trySend(Command.RetryAdmission(token))
        }
    }

    private fun invalidateAdmissionRetry() {
        admissionRetryToken++
        admissionRetryScheduled = false
        admissionRetryDelayMs = 1_000L
    }

    companion object {
        const val OWNER_TOKEN = "realtime"
        private const val MAX_WAITING_SEGMENTS = 64
    }
}
