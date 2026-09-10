package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.live.GeminiTranscribeVad
import java.io.IOException

/** Serializes PCM and speech-end messages without ending the recording or socket. */
internal class GeminiLiveInputTurnBoundary(
    private val dedicatedTranscribe: Boolean,
    private val nowMs: () -> Long,
) {
    private val vad = GeminiTranscribeVad()
    private var closed = false

    @Synchronized
    fun sendAudio(samples: ShortArray, send: () -> Boolean, sendEnd: () -> Boolean) {
        if (closed) throw IOException("Gemini Live input is closed.")
        if (!send()) throw IOException("Gemini Live audio chunk was rejected.")
        if (dedicatedTranscribe && vad.observe(samples, nowMs())) endTurn(sendEnd)
    }

    @Synchronized
    fun poll(sendEnd: () -> Boolean) {
        if (!closed && dedicatedTranscribe && vad.pollEnd(nowMs())) endTurn(sendEnd)
    }

    @Synchronized
    fun finish(sendEnd: () -> Boolean) {
        if (closed) throw IOException("Gemini Live input is closed.")
        closed = true
        endTurn(sendEnd)
    }

    @Synchronized
    fun cancel() { closed = true }

    private fun endTurn(sendEnd: () -> Boolean) {
        if (!sendEnd()) {
            closed = true
            throw IOException("Gemini Live speech-end message was rejected.")
        }
    }
}
