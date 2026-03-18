package dev.screengoated.toolbox.mobile.service.tts

import android.content.Context
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset
import dev.screengoated.toolbox.mobile.storage.SecureSettingsStore
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import okhttp3.OkHttpClient
import java.util.concurrent.LinkedBlockingDeque
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.atomic.AtomicReference
import kotlin.concurrent.thread

interface TtsRuntimeService {
    val runtimeState: StateFlow<TtsRuntimeState>
    val playbackEvents: SharedFlow<TtsPlaybackEvent>
    val edgeVoiceCatalogState: StateFlow<EdgeVoiceCatalogState>

    fun ensureEdgeVoiceCatalog(force: Boolean = false)

    fun enqueue(request: TtsRequest): Long

    fun enqueueRealtime(request: TtsRequest): Long

    fun interruptAndSpeak(request: TtsRequest): Long

    fun stop()

    fun stopIfActive(requestId: Long)

    fun hasPendingAudio(): Boolean

    fun setRealtimeQueueDepth(depth: Int)
}

class AndroidTtsRuntimeService(
    context: Context,
    httpClient: OkHttpClient,
    private val settingsStore: SecureSettingsStore,
    edgeVoiceCatalogService: EdgeVoiceCatalogService,
) : TtsRuntimeService {
    private val interruptGeneration = AtomicLong(0)
    private val requestCounter = AtomicLong(1)
    private val isPlaying = AtomicBoolean(false)
    private val realtimeQueueDepth = AtomicInteger(0)

    private val workQueue = LinkedBlockingDeque<QueuedRequest>()
    private val playbackQueue = LinkedBlockingDeque<PlaybackRequest>()
    private val activePlayback = AtomicReference<PlaybackRequest?>(null)

    private val mutableState = MutableStateFlow(TtsRuntimeState())
    private val mutableEvents = MutableSharedFlow<TtsPlaybackEvent>(extraBufferCapacity = 64)

    private val audioPlayer = AudioTrackPlayer(context)
    private val languageDetector = DeviceLanguageDetector(context)
    private val mp3Decoder = Mp3Decoder()
    private val geminiProvider = GeminiTtsProvider(httpClient, languageDetector)
    private val edgeProvider = EdgeTtsProvider(httpClient, languageDetector, mp3Decoder)
    private val googleProvider = GoogleTranslateTtsProvider(httpClient, languageDetector, mp3Decoder)
    private val edgeCatalogService = edgeVoiceCatalogService

    override val runtimeState: StateFlow<TtsRuntimeState> = mutableState.asStateFlow()
    override val playbackEvents: SharedFlow<TtsPlaybackEvent> = mutableEvents.asSharedFlow()
    override val edgeVoiceCatalogState: StateFlow<EdgeVoiceCatalogState> = edgeCatalogService.state

    init {
        repeat(WORKER_COUNT) { workerIndex ->
            thread(
                name = "sgt-tts-worker-$workerIndex",
                start = true,
                isDaemon = true,
            ) {
                runWorkerLoop()
            }
        }
        thread(
            name = "sgt-tts-player",
            start = true,
            isDaemon = true,
        ) {
            runPlayerLoop()
        }
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

    override fun interruptAndSpeak(request: TtsRequest): Long {
        return enqueueInternal(request.copy(requestMode = TtsRequestMode.INTERRUPT), forceInterrupt = true)
    }

    override fun stop() {
        interruptGeneration.incrementAndGet()
        workQueue.clear()
        playbackQueue.clear()
        audioPlayer.stopImmediate()
        activePlayback.set(null)
        isPlaying.set(false)
        syncState()
    }

    override fun stopIfActive(requestId: Long) {
        if (runtimeState.value.activeRequestId == requestId) {
            stop()
        }
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
    ): Long {
        val shouldInterrupt = forceInterrupt || shouldInterruptForPriority(request)
        val generation = if (shouldInterrupt) {
            interruptGeneration.incrementAndGet().also {
                workQueue.clear()
                playbackQueue.clear()
                audioPlayer.stopImmediate()
            }
        } else {
            interruptGeneration.get()
        }

        val requestId = requestCounter.getAndIncrement()
        val events = LinkedBlockingDeque<ProviderAudioEvent>()
        val queued = QueuedRequest(
            requestId = requestId,
            generation = generation,
            request = request,
            audioEvents = events,
        )

        workQueue.offer(queued)
        playbackQueue.offer(
            PlaybackRequest(
                requestId = requestId,
                generation = generation,
                request = request,
                audioEvents = events,
            ),
        )
        syncState()
        return requestId
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

    private fun runWorkerLoop() {
        while (true) {
            val job = workQueue.poll(WAIT_TIMEOUT_MS, java.util.concurrent.TimeUnit.MILLISECONDS) ?: continue
            syncState()

            if (job.generation < interruptGeneration.get()) {
                job.audioEvents.offer(ProviderAudioEvent.End)
                continue
            }


            val method = job.request.settingsSnapshot.method
            val apiKey = settingsStore.loadApiKey().trim()
            runCatching {
                when (method) {
                    MobileTtsMethod.GEMINI_LIVE -> geminiProvider.stream(
                        apiKey = apiKey,
                        request = job.request,
                        isStale = { job.generation < interruptGeneration.get() },
                        sink = job.audioEvents,
                    )

                    MobileTtsMethod.EDGE_TTS -> edgeProvider.stream(
                        request = job.request,
                        isStale = { job.generation < interruptGeneration.get() },
                        sink = job.audioEvents,
                    )

                    MobileTtsMethod.GOOGLE_TRANSLATE -> googleProvider.stream(
                        request = job.request,
                        sink = job.audioEvents,
                    )
                }
            }.onFailure { error ->
                job.audioEvents.offer(
                    ProviderAudioEvent.Error(
                        error.message ?: "TTS provider failed.",
                    ),
                )
            }
        }
    }

    private fun runPlayerLoop() {
        while (true) {
            val job = playbackQueue.poll(WAIT_TIMEOUT_MS, java.util.concurrent.TimeUnit.MILLISECONDS) ?: continue
            syncState()

            if (job.generation < interruptGeneration.get()) {
                emitPlaybackEvent(job, TtsCompletionStatus.INTERRUPTED)
                continue
            }

            activePlayback.set(job)
            isPlaying.set(true)
            syncState(activeRequest = job.request)

            var completion = TtsCompletionStatus.COMPLETED
            var done = false

            while (!done) {
                if (job.generation < interruptGeneration.get()) {
                    completion = TtsCompletionStatus.INTERRUPTED
                    audioPlayer.stopImmediate()
                    done = true
                    break
                }

                when (val event = job.audioEvents.poll(WAIT_TIMEOUT_MS, java.util.concurrent.TimeUnit.MILLISECONDS)) {
                    null -> Unit
                    is ProviderAudioEvent.PcmData -> audioPlayer.playPcm24k(
                        pcm24k = event.payload,
                        speedPercent = effectiveSpeedFor(job.request),
                        volumePercent = effectiveVolumeFor(job.request),
                    )

                    is ProviderAudioEvent.End -> {
                        audioPlayer.drain()
                        completion = TtsCompletionStatus.COMPLETED
                        done = true
                    }

                    is ProviderAudioEvent.Error -> {
                        audioPlayer.stopImmediate()
                        completion = TtsCompletionStatus.FAILED
                        done = true
                    }
                }
            }

            activePlayback.set(null)
            isPlaying.set(false)
            syncState()
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
        return if (request.requestMode == TtsRequestMode.REALTIME) {
            request.settingsSnapshot.realtimeVolumePercent.coerceIn(0, 200)
        } else {
            when (request.settingsSnapshot.method) {
                MobileTtsMethod.EDGE_TTS -> (100 + request.settingsSnapshot.edgeSettings.volume).coerceIn(0, 200)
                else -> 100
            }
        }
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
        mutableEvents.tryEmit(
            TtsPlaybackEvent(
                requestId = job.requestId,
                consumer = job.request.consumer,
                ownerToken = job.request.ownerToken,
                completionStatus = status,
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
        private const val WAIT_TIMEOUT_MS = 500L
    }
}
