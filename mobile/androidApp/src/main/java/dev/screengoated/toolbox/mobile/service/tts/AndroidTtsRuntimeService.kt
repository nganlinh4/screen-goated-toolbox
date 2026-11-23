package dev.screengoated.toolbox.mobile.service.tts

import android.content.Context
import dev.screengoated.toolbox.mobile.AppToastBus
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset
import dev.screengoated.toolbox.mobile.model.androidSupportedMethod
import dev.screengoated.toolbox.mobile.storage.SecureSettingsStore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import okhttp3.OkHttpClient
import java.util.concurrent.LinkedBlockingDeque
import java.util.concurrent.Callable
import java.util.concurrent.ExecutionException
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.atomic.AtomicReference
import dev.screengoated.toolbox.mobile.ui.i18n.apiKeyErrorToastText

const val TTS_REQUEST_REJECTED = 0L
const val TTS_REQUEST_CANCELLED = -1L

interface TtsRuntimeService {
    val runtimeState: StateFlow<TtsRuntimeState>
    fun playbackEventsFor(vararg selectors: TtsEventSelector): Flow<TtsPlaybackEvent>
    val edgeVoiceCatalogState: StateFlow<EdgeVoiceCatalogState>

    fun ensureEdgeVoiceCatalog(force: Boolean = false)

    fun enqueue(request: TtsRequest): Long

    fun enqueueRealtime(request: TtsRequest): Long

    fun realtimeContinuationFence(): TtsInterruption

    fun interruptAndSpeak(request: TtsRequest): Long

    /** Replace only an earlier settings preview, leaving other owners untouched. */
    fun enqueueSettingsPreview(request: TtsRequest): Long

    fun stop()

    fun stopIfActive(requestId: Long)

    /** Retire a queued realtime request after its producer has invalidated dispatch. */
    fun retireRealtime(requestId: Long, ownerToken: String)

    fun hasPendingAudio(): Boolean

    fun setRealtimeQueueDepth(depth: Int)
}

class AndroidTtsRuntimeService(
    context: Context,
    httpClient: OkHttpClient,
    private val settingsStore: SecureSettingsStore,
    private val toastBus: AppToastBus,
    edgeVoiceCatalogService: EdgeVoiceCatalogService,
) : TtsRuntimeService {
    private val interruptGeneration = TtsDispatchGeneration()
    private val previewAdmission = SettingsPreviewAdmission()
    private val interruptions = TtsInterruptionTracker()
    private val requestCounter = AtomicLong(1)
    private val isPlaying = AtomicBoolean(false)
    private val realtimeQueueDepth = AtomicInteger(0)

    private val workQueue = LinkedBlockingDeque<QueuedRequest>()
    private val playbackQueue = LinkedBlockingDeque<PlaybackRequest>()
    private val activePlayback = AtomicReference<PlaybackRequest?>(null)
    private val queueControl = Executors.newSingleThreadExecutor { runnable ->
        Thread(runnable, "sgt-tts-queue-control").apply { isDaemon = true }
    }

    private val mutableState = MutableStateFlow(TtsRuntimeState())
    private val eventBus = TtsPlaybackEventBus()
    private val lastCapacityToast = AtomicLong(0)

    private val audioPlayer = AudioTrackPlayer(context)
    private val languageDetector = DeviceLanguageDetector(context)
    private val mp3Decoder = Mp3Decoder()
    private val geminiProvider = GeminiTtsProvider(httpClient, languageDetector)
    private val edgeProvider = EdgeTtsProvider(httpClient, languageDetector, mp3Decoder)
    private val googleProvider = GoogleTranslateTtsProvider(httpClient, languageDetector, mp3Decoder)
    private val edgeCatalogService = edgeVoiceCatalogService

    override val runtimeState: StateFlow<TtsRuntimeState> = mutableState.asStateFlow()
    override fun playbackEventsFor(vararg selectors: TtsEventSelector): Flow<TtsPlaybackEvent> =
        eventBus.eventsFor(*selectors)
    override val edgeVoiceCatalogState: StateFlow<EdgeVoiceCatalogState> = edgeCatalogService.state

    init {
        // Pre-connect Gemini TTS WebSocket for instant first use
        val warmUpKey = settingsStore.loadApiKey().trim()
        android.util.Log.d("TTS-Timing", "Init warmUp: key=${if (warmUpKey.isBlank()) "BLANK" else "present(${warmUpKey.length}chars)"}")
        if (warmUpKey.isNotBlank()) {
            geminiProvider.warmUp(warmUpKey)
        }

        repeat(WORKER_COUNT) { workerIndex ->
            startTtsQueueConsumer("sgt-tts-worker-$workerIndex", workQueue, ::runWorkerJob)
        }
        startTtsQueueConsumer("sgt-tts-player", playbackQueue, ::runPlaybackJob)
    }

    override fun ensureEdgeVoiceCatalog(force: Boolean) {
        edgeCatalogService.ensureLoaded(force)
    }

    override fun enqueue(request: TtsRequest): Long {
        return enqueueInternal(request.copy(requestMode = TtsRequestMode.NORMAL))
    }

    override fun enqueueRealtime(request: TtsRequest): Long {
        return enqueueInternal(request.copy(requestMode = TtsRequestMode.REALTIME))
    }

    override fun realtimeContinuationFence(): TtsInterruption = interruptions.continuationFence()

    override fun interruptAndSpeak(request: TtsRequest): Long {
        return enqueueInternal(request.copy(requestMode = TtsRequestMode.INTERRUPT), forceInterrupt = true)
    }

    override fun enqueueSettingsPreview(request: TtsRequest): Long {
        require(request.consumer == TtsConsumer.SETTINGS_PREVIEW)
        return enqueueInternal(request.copy(requestMode = TtsRequestMode.NORMAL), replacePreview = true)
    }

    override fun stop() = control { stopInternal() }

    private fun stopInternal() {
        interruptQueuedPlayback(replacementId = null)
        activePlayback.set(null)
        isPlaying.set(false)
        syncState()
    }

    private fun interruptQueuedPlayback(replacementId: Long?) {
        interruptions.interrupt(replacementId)
        interruptGeneration.advance()
        workQueue.clear()
        retireQueuedPlayback(playbackQueue) { emitPlaybackEvent(it, TtsCompletionStatus.INTERRUPTED) }
        audioPlayer.stopImmediate()
    }

    override fun stopIfActive(requestId: Long) = control {
        if (runtimeState.value.activeRequestId == requestId) {
            stopInternal()
        }
    }

    override fun retireRealtime(requestId: Long, ownerToken: String) = control {
        fun matches(id: Long, request: TtsRequest): Boolean =
            matchesRealtimeOwner(requestId, ownerToken, id, request.consumer, request.ownerToken)
        workQueue.removeIf { matches(it.requestId, it.request) }
        retireMatchingQueuedPlayback(playbackQueue, { matches(it.requestId, it.request) }) {
            emitPlaybackEvent(it, TtsCompletionStatus.INTERRUPTED)
        }
        syncState()
    }

    override fun hasPendingAudio(): Boolean {
        return isPlaying.get() || workQueue.isNotEmpty() || playbackQueue.isNotEmpty()
    }

    override fun setRealtimeQueueDepth(depth: Int) {
        realtimeQueueDepth.set(depth.coerceAtLeast(0))
        val active = activePlayback.get()
        if (active?.request?.consumer == TtsConsumer.REALTIME) {
            syncState(activeRequest = active.request)
        }
    }

    private fun enqueueInternal(
        request: TtsRequest,
        forceInterrupt: Boolean = false,
        replacePreview: Boolean = false,
    ): Long = control {
        if (request.resumeAfterInterruption?.let { !interruptions.resumeAllowed(it) } == true) {
            return@control TTS_REQUEST_CANCELLED
        }
        val requestId = requestCounter.getAndIncrement()
        if (!eventBus.reserve(requestId, request.consumer, request.ownerToken)) {
            runCatching { showCapacityToast() }
            return@control TTS_REQUEST_REJECTED
        }
        var accepted = false
        var registered = false
        try {
        val shouldInterrupt = forceInterrupt || (!replacePreview && shouldInterruptForPriority(request))
        val generation = if (shouldInterrupt) {
            interruptQueuedPlayback(replacementId = requestId)
            interruptGeneration.current()
        } else {
            interruptGeneration.current()
        }

        val ownedRequest = if (replacePreview) {
            previewAdmission.admit(request)
        } else {
            request
        }
        val guardedRequest = ownedRequest.withDispatchGeneration(interruptGeneration, generation)
        val events = LinkedBlockingDeque<ProviderAudioEvent>()
        val queued = QueuedRequest(
            requestId = requestId,
            generation = generation,
            request = guardedRequest,
            audioEvents = events,
        )

        android.util.Log.d("TTS-Timing", "═══ ENQUEUE requestId=$requestId method=${request.settingsSnapshot.method} t=${android.os.SystemClock.elapsedRealtime()}")
        check(interruptions.register(requestId)) { "TTS interruption capacity invariant failed" }
        registered = true
        workQueue.offer(queued)
        playbackQueue.offer(
            PlaybackRequest(
                requestId = requestId,
                generation = generation,
                request = guardedRequest,
                audioEvents = events,
            ),
        )
        accepted = true
        runCatching { syncState() }
        requestId
        } catch (error: Throwable) {
            if (!accepted) {
                if (registered) interruptions.terminal(requestId)
                eventBus.abandon(requestId)
            }
            throw error
        }
    }

    private fun showCapacityToast() {
        val now = android.os.SystemClock.elapsedRealtime()
        val previous = lastCapacityToast.get()
        if (previous != 0L && now - previous < 3_000L) return
        if (!lastCapacityToast.compareAndSet(previous, now.coerceAtLeast(1))) return
        val message = when (settingsStore.loadUiPreferences().uiLanguage) {
            "vi" -> "Hàng đợi đọc đã đầy. Vui lòng thử lại."
            "ko" -> "음성 대기열이 가득 찼습니다. 다시 시도하세요."
            else -> "The speech queue is full. Please try again."
        }
        toastBus.show(message)
    }

    private fun <T> control(action: () -> T): T = try {
        queueControl.submit(Callable(action)).get()
    } catch (error: ExecutionException) {
        throw error.cause ?: error
    }

    private fun shouldInterruptForPriority(request: TtsRequest): Boolean {
        if (request.requestMode == TtsRequestMode.INTERRUPT) {
            return true
        }
        if (request.priority != TtsPriority.PREVIEW) {
            return false
        }
        return isPlaying.get() || playbackQueue.isNotEmpty() || workQueue.isNotEmpty()
    }

    private fun runWorkerJob(job: QueuedRequest) {
        try {
            syncState()

            if (job.generation < interruptGeneration.current() || !job.request.canDispatch()) {
                job.audioEvents.offer(ProviderAudioEvent.End)
                return
            }


            android.util.Log.d("TTS-Timing", "Worker picked up job: requestId=${job.requestId} method=${job.request.settingsSnapshot.method}")
            val method = job.request.settingsSnapshot.method.androidSupportedMethod()
            val apiKey = settingsStore.loadApiKey().trim()
            if (method == MobileTtsMethod.GEMINI_LIVE && apiKey.isBlank()) {
                job.audioEvents.offer(ProviderAudioEvent.Error("NO_API_KEY:google"))
                return
            }
            runCatching {
                when (method) {
                    MobileTtsMethod.GEMINI_LIVE -> geminiProvider.stream(
                        apiKey = apiKey,
                        request = job.request,
                        isStale = { job.generation < interruptGeneration.current() || !job.request.canDispatch() },
                        sink = job.audioEvents,
                    )

                    MobileTtsMethod.EDGE_TTS -> edgeProvider.stream(
                        request = job.request,
                        isStale = { job.generation < interruptGeneration.current() || !job.request.canDispatch() },
                        sink = job.audioEvents,
                    )

                    MobileTtsMethod.GOOGLE_TRANSLATE -> googleProvider.stream(
                        request = job.request,
                        isStale = { job.generation < interruptGeneration.current() || !job.request.canDispatch() },
                        sink = job.audioEvents,
                    )
                }
            }.also {
                // Pre-connect next warm socket after each Gemini request
                if (method == MobileTtsMethod.GEMINI_LIVE && apiKey.isNotBlank()) {
                    geminiProvider.warmUp(apiKey)
                }
            }.onFailure { error ->
                job.audioEvents.offer(
                    ProviderAudioEvent.Error(
                        error.message ?: "TTS provider failed.",
                    ),
                )
            }
        } catch (error: Exception) {
            job.audioEvents.offer(ProviderAudioEvent.Error(error.message ?: "TTS provider failed."))
            if (error is InterruptedException) Thread.currentThread().interrupt()
        }
    }

    private fun runPlaybackJob(job: PlaybackRequest) {
        var completion = TtsCompletionStatus.FAILED
        var firstPlayLogged = false
        var writeInProgress = false
        try {
            syncState()

            if (job.generation < interruptGeneration.current() || !job.request.canDispatch()) {
                completion = TtsCompletionStatus.INTERRUPTED
                return
            }

            activePlayback.set(job)
            isPlaying.set(true)
            syncState(activeRequest = job.request)
            val playerStart = android.os.SystemClock.elapsedRealtime()
            android.util.Log.d("TTS-Timing", "  Player picked up requestId=${job.requestId} t=$playerStart")

            completion = TtsCompletionStatus.COMPLETED
            var done = false

            while (!done) {
                if (job.generation < interruptGeneration.current() || !job.request.canDispatch()) {
                    completion = if (firstPlayLogged) TtsCompletionStatus.PARTIAL else TtsCompletionStatus.INTERRUPTED
                    audioPlayer.stopImmediate()
                    done = true
                    break
                }

                val event = job.audioEvents.poll(PLAYBACK_POLL_MS, java.util.concurrent.TimeUnit.MILLISECONDS)
                if (job.generation < interruptGeneration.current() || !job.request.canDispatch()) {
                    completion = if (firstPlayLogged) TtsCompletionStatus.PARTIAL else TtsCompletionStatus.INTERRUPTED
                    audioPlayer.stopImmediate()
                    break
                }
                when (event) {
                    null -> Unit
                    is ProviderAudioEvent.PcmData -> {
                        val current = { job.generation >= interruptGeneration.current() && job.request.canDispatch() }
                        writeInProgress = true
                        val writeResult = audioPlayer.playPcm24kInterruptibly(
                            pcm24k = event.payload,
                            speedPercent = effectiveSpeedFor(job.request),
                            volumePercent = effectiveVolumeFor(job.request),
                            isCurrent = current,
                            dispatch = job.request::dispatchIfCurrent,
                        )
                        writeInProgress = false
                        when (writeResult) {
                            PlaybackWriteResult.COMPLETE -> {
                                if (!firstPlayLogged) {
                                    firstPlayLogged = true
                                    android.util.Log.d("TTS-Timing", "  FIRST PLAYBACK: ${android.os.SystemClock.elapsedRealtime() - playerStart}ms after player pickup")
                                }
                            }
                            PlaybackWriteResult.NO_OUTPUT -> Unit
                            PlaybackWriteResult.INTERRUPTED -> { completion = if (firstPlayLogged) TtsCompletionStatus.PARTIAL else TtsCompletionStatus.INTERRUPTED; done = true }
                            PlaybackWriteResult.PARTIAL -> { completion = TtsCompletionStatus.PARTIAL; done = true }
                            PlaybackWriteResult.FAILED -> { completion = if (firstPlayLogged) TtsCompletionStatus.PARTIAL else TtsCompletionStatus.FAILED; done = true }
                        }
                    }

                    is ProviderAudioEvent.End -> {
                        val current = { job.generation >= interruptGeneration.current() && job.request.canDispatch() }
                        writeInProgress = true
                        val finalWrite = audioPlayer.finishPcm24kInterruptibly(
                            volumePercent = effectiveVolumeFor(job.request),
                            isCurrent = current,
                            dispatch = job.request::dispatchIfCurrent,
                        )
                        writeInProgress = false
                        if (finalWrite == PlaybackWriteResult.COMPLETE) firstPlayLogged = true
                        completion = when (finalWrite) {
                            PlaybackWriteResult.INTERRUPTED -> if (firstPlayLogged) TtsCompletionStatus.PARTIAL else TtsCompletionStatus.INTERRUPTED
                            PlaybackWriteResult.PARTIAL -> TtsCompletionStatus.PARTIAL
                            PlaybackWriteResult.FAILED -> if (firstPlayLogged) TtsCompletionStatus.PARTIAL else TtsCompletionStatus.FAILED
                            PlaybackWriteResult.COMPLETE, PlaybackWriteResult.NO_OUTPUT -> finishTtsPlayback(
                                hasAudio = firstPlayLogged || finalWrite == PlaybackWriteResult.COMPLETE,
                                drain = { audioPlayer.drain(isCurrent = current) },
                                stop = audioPlayer::stopImmediate,
                                isCurrent = current,
                            )
                        }
                        done = true
                    }

                    is ProviderAudioEvent.Error -> {
                        apiKeyErrorToastText(event.message, settingsStore.loadUiPreferences().uiLanguage)?.let(toastBus::show)
                        audioPlayer.stopImmediate()
                        completion = if (firstPlayLogged) TtsCompletionStatus.PARTIAL else TtsCompletionStatus.FAILED
                        done = true
                    }
                }
            }
        } catch (error: Exception) {
            completion = if (firstPlayLogged || writeInProgress) TtsCompletionStatus.PARTIAL else TtsCompletionStatus.FAILED
            android.util.Log.e("TTS", "Playback failed", error)
            runCatching { audioPlayer.stopImmediate() }
            if (error is InterruptedException) Thread.currentThread().interrupt()
        } finally {
            activePlayback.compareAndSet(job, null)
            isPlaying.set(false)
            runCatching { syncState() }
            emitPlaybackEvent(job, completion)
        }
    }

    private fun effectiveSpeedFor(request: TtsRequest): Int {
        if (request.requestMode != TtsRequestMode.REALTIME) {
            return when (request.settingsSnapshot.method) {
                MobileTtsMethod.GOOGLE_TRANSLATE -> when (request.settingsSnapshot.speedPreset) {
                    MobileTtsSpeedPreset.SLOW -> 75
                    MobileTtsSpeedPreset.NORMAL, MobileTtsSpeedPreset.FAST -> 100
                }

                else -> 100
            }
        }

        val base = request.settingsSnapshot.realtimeSpeedPercent.coerceIn(50, 200)
        val boost = if (request.settingsSnapshot.realtimeAutoSpeed) {
            (realtimeQueueDepth.get() * 15).coerceAtMost(60)
        } else {
            0
        }
        return (base + boost).coerceAtMost(200)
    }

    private fun effectiveVolumeFor(request: TtsRequest): Int {
        return playbackVolumePercent(request)
    }

    private fun syncState(activeRequest: TtsRequest? = activePlayback.get()?.request) {
        mutableState.value = TtsRuntimeState(
            isPlaying = isPlaying.get(),
            activeRequestId = activePlayback.get()?.requestId,
            activeConsumer = activeRequest?.consumer,
            pendingWorkCount = workQueue.size,
            pendingPlaybackCount = playbackQueue.size,
            currentRealtimeEffectiveSpeed = if (activeRequest?.consumer == TtsConsumer.REALTIME) {
                effectiveSpeedFor(activeRequest)
            } else {
                mutableState.value.currentRealtimeEffectiveSpeed
            },
        )
    }

    private fun emitPlaybackEvent(
        job: PlaybackRequest,
        status: TtsCompletionStatus,
    ) {
        val interruptedBy = interruptions.terminal(job.requestId)
        eventBus.publish(
            TtsPlaybackEvent(
                requestId = job.requestId,
                consumer = job.request.consumer,
                ownerToken = job.request.ownerToken,
                completionStatus = status,
                interruptedBy = interruptedBy,
            ),
        )
    }

    private data class QueuedRequest(
        val requestId: Long,
        val generation: Long,
        val request: TtsRequest,
        val audioEvents: LinkedBlockingDeque<ProviderAudioEvent>,
    )

    private data class PlaybackRequest(
        val requestId: Long,
        val generation: Long,
        val request: TtsRequest,
        val audioEvents: LinkedBlockingDeque<ProviderAudioEvent>,
    )

    private companion object {
        private const val WORKER_COUNT = 2
        private const val PLAYBACK_POLL_MS = 20L
    }
}

internal fun playbackVolumePercent(request: TtsRequest): Int {
    return if (request.requestMode == TtsRequestMode.REALTIME) {
        request.settingsSnapshot.realtimeVolumePercent.coerceIn(0, 200)
    } else {
        100
    }
}
