package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.widget.Toast
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.service.tts.TtsCompletionStatus
import dev.screengoated.toolbox.mobile.service.tts.TtsConsumer
import dev.screengoated.toolbox.mobile.service.tts.TtsPlaybackEvent
import dev.screengoated.toolbox.mobile.service.tts.TtsPriority
import dev.screengoated.toolbox.mobile.service.tts.TtsRequest
import dev.screengoated.toolbox.mobile.service.tts.TtsRequestMode
import dev.screengoated.toolbox.mobile.service.tts.TtsRuntimeService
import dev.screengoated.toolbox.mobile.service.tts.TtsRequestSettingsSnapshot
import java.util.concurrent.atomic.AtomicLong

internal class PresetAutoSpeakCoordinator(
    private val context: Context,
    private val ttsRuntimeService: TtsRuntimeService,
    private val snapshotProvider: () -> TtsRequestSettingsSnapshot,
    private val uiLanguage: () -> String,
) {
    private val handler = Handler(Looper.getMainLooper())
    private val requestCounter = AtomicLong(1L)
    private val pendingRequests = linkedMapOf<String, PendingAutoSpeak>()

    fun schedule(
        text: String,
        blockIdx: Int,
    ) {
        val trimmed = text.trim()
        if (trimmed.isEmpty()) {
            return
        }
        val ownerToken = "autospeak_block_${blockIdx}_${requestCounter.getAndIncrement()}"
        handler.postDelayed({
            enqueue(ownerToken = ownerToken, text = trimmed, retryCount = 0)
        }, AUTO_SPEAK_DELAY_MS)
    }

    fun handlePlaybackEvent(event: TtsPlaybackEvent) {
        if (event.consumer != TtsConsumer.AUTO_SPEAK) {
            return
        }
        val pending = pendingRequests[event.ownerToken] ?: return
        when (event.completionStatus) {
            TtsCompletionStatus.COMPLETED,
            TtsCompletionStatus.INTERRUPTED,
            -> pendingRequests.remove(event.ownerToken)

            TtsCompletionStatus.FAILED -> {
                if (pending.retryCount >= MAX_AUTO_SPEAK_RETRIES) {
                    pendingRequests.remove(event.ownerToken)
                    Log.w(TAG, "Auto-speak failed after retry: owner=${event.ownerToken}")
                    Toast.makeText(
                        context,
                        localized(
                            "Could not speak the preset result.",
                            "Không thể đọc kết quả preset.",
                            "프리셋 결과를 읽지 못했습니다.",
                        ),
                        Toast.LENGTH_SHORT,
                    ).show()
                    return
                }
                handler.postDelayed({
                    enqueue(
                        ownerToken = event.ownerToken,
                        text = pending.text,
                        retryCount = pending.retryCount + 1,
                    )
                }, AUTO_SPEAK_RETRY_DELAY_MS)
            }
        }
    }

    fun clear() {
        pendingRequests.clear()
        handler.removeCallbacksAndMessages(null)
    }

    private fun enqueue(
        ownerToken: String,
        text: String,
        retryCount: Int,
    ) {
        val snapshot = snapshotProvider()
        if (snapshot.method == MobileTtsMethod.EDGE_TTS) {
            ttsRuntimeService.ensureEdgeVoiceCatalog()
        }
        pendingRequests[ownerToken] = PendingAutoSpeak(text = text, retryCount = retryCount)
        ttsRuntimeService.enqueue(
            TtsRequest(
                text = text,
                consumer = TtsConsumer.AUTO_SPEAK,
                priority = TtsPriority.USER,
                requestMode = TtsRequestMode.NORMAL,
                settingsSnapshot = snapshot,
                ownerToken = ownerToken,
            ),
        )
    }

    private fun localized(
        en: String,
        vi: String,
        ko: String,
    ): String = when (uiLanguage()) {
        "vi" -> vi
        "ko" -> ko
        else -> en
    }

    private data class PendingAutoSpeak(
        val text: String,
        val retryCount: Int,
    )

    private companion object {
        private const val TAG = "PresetAutoSpeak"
        private const val AUTO_SPEAK_DELAY_MS = 200L
        private const val AUTO_SPEAK_RETRY_DELAY_MS = 250L
        private const val MAX_AUTO_SPEAK_RETRIES = 1
    }
}
