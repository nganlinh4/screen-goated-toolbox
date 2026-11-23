package dev.screengoated.toolbox.mobile.service.preset

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.widget.Toast
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.preset.PresetEffectLease
import dev.screengoated.toolbox.mobile.service.tts.TtsCompletionStatus
import dev.screengoated.toolbox.mobile.service.tts.TTS_REQUEST_REJECTED
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
        lease: PresetEffectLease,
    ) {
        val trimmed = text.trim()
        if (trimmed.isEmpty()) {
            return
        }
        val ownerToken = "${AUTO_SPEAK_OWNER_PREFIX}${blockIdx}_${requestCounter.getAndIncrement()}"
        handler.postDelayed({
            enqueue(ownerToken = ownerToken, text = trimmed, retryCount = 0, lease = lease)
        }, AUTO_SPEAK_DELAY_MS)
    }

    fun handlePlaybackEvent(event: TtsPlaybackEvent) {
        if (event.consumer != TtsConsumer.AUTO_SPEAK) {
            return
        }
        val pending = pendingRequests[event.ownerToken] ?: return
        if (!pending.lease.isCurrent()) {
            pendingRequests.remove(event.ownerToken)
            return
        }
        when (event.completionStatus) {
            TtsCompletionStatus.COMPLETED,
            TtsCompletionStatus.INTERRUPTED,
            TtsCompletionStatus.PARTIAL,
            -> pendingRequests.remove(event.ownerToken)

            TtsCompletionStatus.FAILED -> {
                if (pending.retryCount >= MAX_AUTO_SPEAK_RETRIES) {
                    pendingRequests.remove(event.ownerToken)
                    Log.w(TAG, "Auto-speak failed after retry: owner=${event.ownerToken}")
                    pending.lease.runIfCurrent { Toast.makeText(
                        context,
                        localized(
                            "Could not speak the preset result.",
                            "Không thể đọc kết quả preset.",
                            "프리셋 결과를 읽지 못했습니다.",
                        ),
                        Toast.LENGTH_SHORT,
                    ).show() }
                    return
                }
                handler.postDelayed({
                    enqueue(
                        ownerToken = event.ownerToken,
                        text = pending.text,
                        retryCount = pending.retryCount + 1,
                        lease = pending.lease,
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
        lease: PresetEffectLease,
    ) {
        if (!lease.isCurrent()) {
            pendingRequests.remove(ownerToken)
            return
        }
        val snapshot = snapshotProvider()
        if (snapshot.method == MobileTtsMethod.EDGE_TTS) {
            ttsRuntimeService.ensureEdgeVoiceCatalog()
        }
        if (isMissingGeminiKey()) {
            lease.runIfCurrent {
                val message = localized(
                    "Add your Gemini API key before using Gemini TTS.",
                    "Hãy thêm Gemini API key của bạn trước khi dùng Gemini TTS.",
                    "Gemini API 키를 추가한 뒤 Gemini TTS를 사용하세요.",
                )
                (context.applicationContext as SgtMobileApplication).appContainer.toastBus.show(message)
            }
            return
        }
        pendingRequests[ownerToken] = PendingAutoSpeak(text = text, retryCount = retryCount, lease = lease)
        val requestId = enqueuePresetSpeechIfCurrent(lease) {
            ttsRuntimeService.enqueue(
                TtsRequest(
                    text = text,
                    consumer = TtsConsumer.AUTO_SPEAK,
                    priority = TtsPriority.USER,
                    requestMode = TtsRequestMode.NORMAL,
                    settingsSnapshot = snapshot,
                    ownerToken = ownerToken,
                    isCurrent = lease::isCurrent,
                    runIfCurrent = lease::runIfCurrent,
                ),
            )
        }
        if (requestId == TTS_REQUEST_REJECTED || !lease.isCurrent()) {
            pendingRequests.remove(ownerToken)
        }
    }

    private fun localized(
        en: String,
        vi: String,
        ko: String,
    ): String = overlayLocalized(uiLanguage(), en, vi, ko)

    private fun isMissingGeminiKey(): Boolean {
        val settings = snapshotProvider()
        if (settings.method != MobileTtsMethod.GEMINI_LIVE) {
            return false
        }
        val appContainer = (context.applicationContext as SgtMobileApplication).appContainer
        return appContainer.repository.currentApiKey().isBlank()
    }

    private data class PendingAutoSpeak(
        val text: String,
        val retryCount: Int,
        val lease: PresetEffectLease,
    )

    private companion object {
        private const val TAG = "PresetAutoSpeak"
        private const val AUTO_SPEAK_DELAY_MS = 200L
        private const val AUTO_SPEAK_RETRY_DELAY_MS = 250L
        private const val MAX_AUTO_SPEAK_RETRIES = 1
    }
}

internal const val AUTO_SPEAK_OWNER_PREFIX = "autospeak_block_"

internal fun enqueuePresetSpeechIfCurrent(lease: PresetEffectLease, enqueue: () -> Long): Long =
    if (lease.isCurrent()) enqueue() else TTS_REQUEST_REJECTED
