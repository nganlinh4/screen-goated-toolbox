package dev.screengoated.toolbox.mobile.service.tts

import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.RealtimeTtsSettings
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch

class RealtimeTtsCoordinator(
    private val runtime: TtsRuntimeService,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val pendingOffsets = LinkedHashMap<Long, Int>()
    private var playbackEventsJob: Job? = null
    private var spokenLength: Int = 0
    private var queuedLength: Int = 0

    init {
        playbackEventsJob = scope.launch {
            runtime.playbackEvents.collectLatest { event ->
                if (event.consumer != TtsConsumer.REALTIME || event.ownerToken != OWNER_TOKEN) {
                    return@collectLatest
                }
                when (event.completionStatus) {
                    TtsCompletionStatus.COMPLETED -> {
                        val completedOffset = pendingOffsets.remove(event.requestId)
                        if (completedOffset != null) {
                            spokenLength = maxOf(spokenLength, completedOffset)
                            queuedLength = maxOf(queuedLength, spokenLength)
                        }
                    }

                    TtsCompletionStatus.INTERRUPTED,
                    TtsCompletionStatus.FAILED,
                    -> {
                        pendingOffsets.clear()
                        queuedLength = spokenLength
                    }
                }
                runtime.setRealtimeQueueDepth(pendingOffsets.size)
            }
        }
    }

    fun update(
        committedText: String,
        targetLanguage: String,
        globalSettings: MobileGlobalTtsSettings,
        realtimeSettings: RealtimeTtsSettings,
        translationVisible: Boolean,
    ) {
        if (!translationVisible) {
            stop()
            return
        }

        if (!realtimeSettings.enabled) {
            stopAndReset()
            return
        }

        val normalized = committedText.trimEnd()
        if (normalized.isBlank()) {
            resetOffsets()
            runtime.setRealtimeQueueDepth(0)
            return
        }

        if (normalized.length < spokenLength || normalized.length < queuedLength) {
            resetOffsets()
        }

        if (spokenLength == 0 && queuedLength == 0 && normalized.length > 50) {
            val boundary = normalized.dropLast(1).lastIndexOfAny(charArrayOf('.', '?', '!', '\n'))
            if (boundary > 0) {
                val offset = boundary + 1
                spokenLength = offset
                queuedLength = offset
            }
        }

        if (normalized.length <= queuedLength) {
            runtime.setRealtimeQueueDepth(pendingOffsets.size)
            return
        }

        val nextSegment = normalized.substring(queuedLength)
        if (nextSegment.isBlank()) {
            queuedLength = normalized.length
            runtime.setRealtimeQueueDepth(pendingOffsets.size)
            return
        }

        val requestId = runtime.enqueueRealtime(
            TtsRequest(
                text = nextSegment,
                consumer = TtsConsumer.REALTIME,
                priority = TtsPriority.REALTIME,
                requestMode = TtsRequestMode.REALTIME,
                settingsSnapshot = globalSettings.toRuntimeSnapshot(
                    targetLanguage = targetLanguage,
                    realtimeSettings = realtimeSettings,
                ),
                ownerToken = OWNER_TOKEN,
            ),
        )
        queuedLength = normalized.length
        pendingOffsets[requestId] = normalized.length
        runtime.setRealtimeQueueDepth(pendingOffsets.size)
    }

    fun stop() {
        runtime.stop()
        pendingOffsets.clear()
        queuedLength = spokenLength
        runtime.setRealtimeQueueDepth(0)
    }

    fun stopAndReset() {
        runtime.stop()
        resetOffsets()
        runtime.setRealtimeQueueDepth(0)
    }

    private fun resetOffsets() {
        pendingOffsets.clear()
        spokenLength = 0
        queuedLength = 0
    }

    companion object {
        const val OWNER_TOKEN = "realtime"
    }
}
