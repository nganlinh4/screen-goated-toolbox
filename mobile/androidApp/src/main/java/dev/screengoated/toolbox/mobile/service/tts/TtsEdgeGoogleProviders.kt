package dev.screengoated.toolbox.mobile.service.tts

import okhttp3.OkHttpClient
import okhttp3.Call
import okhttp3.Callback
import okhttp3.Request
import okhttp3.Response
import java.io.IOException
import java.net.URLEncoder
import java.nio.charset.StandardCharsets
import java.util.UUID
import java.util.concurrent.LinkedBlockingDeque
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit

private const val DEFAULT_EDGE_VOICE = "en-US-AriaNeural"

/** Null means retired before dispatch; false means the transport rejected the payload. */
internal fun dispatchTtsPayload(request: TtsRequest, isStale: () -> Boolean, send: () -> Boolean): Boolean? {
    var accepted: Boolean? = null
    request.dispatchIfCurrent {
        if (!isStale()) accepted = send()
    }
    return accepted
}

internal class EdgeTtsProvider(
    private val httpClient: OkHttpClient,
    private val languageDetector: DeviceLanguageDetector,
    private val mp3Decoder: Mp3Decoder,
) {
    fun stream(
        request: TtsRequest,
        isStale: () -> Boolean,
        sink: LinkedBlockingDeque<ProviderAudioEvent>,
    ) {
        val voiceName = resolveVoice(request)
        val settings = request.settingsSnapshot.edgeSettings
        val connectionId = UUID.randomUUID().toString().replace("-", "")

        val secMsGec = generateSecMsGec()
        val socketRequest = Request.Builder()
            .url("$EDGE_WS_ENDPOINT?TrustedClientToken=$EDGE_TRUSTED_TOKEN&ConnectionId=$connectionId&Sec-MS-GEC=$secMsGec&Sec-MS-GEC-Version=$SEC_MS_GEC_VERSION")
            .header("Origin", EDGE_ORIGIN)
            .header("Pragma", "no-cache")
            .header("Cache-Control", "no-cache")
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/$CHROMIUM_FULL_VERSION Safari/537.36 Edg/$CHROMIUM_FULL_VERSION")
            .build()

        val lease = EdgeConnectionLease.acquire()
        if (lease == null) {
            if (!isStale()) sink.offer(ProviderAudioEvent.Error("Edge TTS connection capacity is busy."))
            return
        }
        var ownedSession: EdgeSocketSession? = null
        try {
            val admitted = dispatchTtsPayload(request, isStale) {
                ownedSession = EdgeSocketSession(httpClient, socketRequest, lease)
                true
            }
            if (admitted == null) {
                lease.release()
                return
            }
            val session = checkNotNull(ownedSession)
            session.use {
                when (val opening = session.state.awaitOpen(isStale)) {
                    is EdgeOutcome.Failure -> { if (!isStale()) sink.offer(ProviderAudioEvent.Error(opening.message)); return }
                    is EdgeOutcome.Cancelled -> return
                    else -> Unit
                }
                val configMessage =
                    "X-Timestamp:${edgeHeaderTimestamp()}\r\n" +
                        "Content-Type:application/json; charset=utf-8\r\n" +
                        "Path:speech.config\r\n\r\n" +
                        "{\"context\":{\"synthesis\":{\"audio\":{\"metadataoptions\":{\"sentenceBoundaryEnabled\":\"false\",\"wordBoundaryEnabled\":\"false\"},\"outputFormat\":\"audio-24khz-48kbitrate-mono-mp3\"}}}}"

                if (!sendEdgePayload(session, request, isStale, sink, configMessage, false)) return
                val ssml = buildEdgeSsml(
                    text = request.text,
                    voiceName = voiceName,
                    pitch = settings.pitch,
                    rate = settings.rate,
                    volume = settings.volume,
                )
                val ssmlMessage =
                    "X-RequestId:$connectionId\r\n" +
                        "Content-Type:application/ssml+xml\r\n" +
                        "X-Timestamp:${edgeSsmlTimestamp()}Z\r\n" +
                        "Path:ssml\r\n\r\n$ssml"
                if (!sendEdgePayload(session, request, isStale, sink, ssmlMessage, true)) return

                when (val result = session.state.awaitResult(isStale)) {
                    is EdgeOutcome.Complete -> emitDecodedMp3(result.mp3, request, isStale, sink)
                    is EdgeOutcome.Failure -> if (!isStale()) sink.offer(ProviderAudioEvent.Error(result.message))
                    EdgeOutcome.Cancelled -> Unit
                }
            }
        } catch (error: Exception) {
            if (!isStale()) sink.offer(ProviderAudioEvent.Error("Edge TTS connection failed."))
            if (error is InterruptedException) Thread.currentThread().interrupt()
        } finally {
            if (ownedSession == null) lease.release() else ownedSession.close()
        }
    }

    private fun sendEdgePayload(
        session: EdgeSocketSession,
        request: TtsRequest,
        isStale: () -> Boolean,
        sink: LinkedBlockingDeque<ProviderAudioEvent>,
        payload: String,
        requestPayload: Boolean,
    ): Boolean {
        val outcome = session.state.currentOutcome()
        if (outcome is EdgeOutcome.Failure) {
            if (!isStale()) sink.offer(ProviderAudioEvent.Error(outcome.message))
            return false
        }
        if (outcome != null) return false
        var sent: Boolean? = null
        val admitted = request.dispatchIfCurrent {
            if (!isStale()) sent = session.state.sendIfActive(requestPayload) { session.send(payload) }
        }
        if (!admitted || isStale()) return false
        if (sent != true) {
            val failure = session.state.currentOutcome() as? EdgeOutcome.Failure
            sink.offer(ProviderAudioEvent.Error(failure?.message ?: "Edge TTS payload was rejected."))
            return false
        }
        return true
    }

    private fun resolveVoice(request: TtsRequest): String {
        return resolveEdgeVoice(request) { languageDetector.detectIso639_1(request.text) }
    }

    private fun emitDecodedMp3(
        mp3: ByteArray,
        request: TtsRequest,
        isStale: () -> Boolean,
        sink: LinkedBlockingDeque<ProviderAudioEvent>,
    ) {
        if (isStale()) return
        val pcm = try {
            mp3Decoder.decodeToMonoPcm24k(mp3)
        } catch (e: Exception) {
            if (!isStale()) sink.offer(ProviderAudioEvent.Error("Edge TTS MP3 decode failed: ${e.message}"))
            return
        }
        if (isStale()) return
        if (pcm.isEmpty()) {
            sink.offer(ProviderAudioEvent.Error("Edge TTS did not return playable audio."))
            return
        }
        for (chunk in chunkBytes(pcm, CHUNK_BYTES)) {
            if (dispatchTtsPayload(request, isStale) { sink.offer(ProviderAudioEvent.PcmData(chunk)) } != true) return
        }
        dispatchTtsPayload(request, isStale) { sink.offer(ProviderAudioEvent.End) }
    }

    private fun buildEdgeSsml(
        text: String,
        voiceName: String,
        pitch: Int,
        rate: Int,
        volume: Int,
    ): String {
        val pitchString = if (pitch >= 0) "+${pitch}Hz" else "${pitch}Hz"
        val rateString = if (rate >= 0) "+${rate}%" else "${rate}%"
        val volumeString = if (volume >= 0) "+${volume}%" else "${volume}%"
        val escaped = text
            .replace("&", "&amp;")
            .replace("<", "&lt;")
            .replace(">", "&gt;")
            .replace("\"", "&quot;")
            .replace("'", "&apos;")

        return "<speak version='1.0' xmlns='http://www.w3.org/2001/10/synthesis' xml:lang='en-US'>" +
            "<voice name='$voiceName'>" +
            "<prosody pitch='$pitchString' rate='$rateString' volume='$volumeString'>$escaped</prosody>" +
            "</voice></speak>"
    }

    private fun edgeHeaderTimestamp(): String {
        return java.time.ZonedDateTime.now(java.time.ZoneOffset.UTC)
            .format(java.time.format.DateTimeFormatter.ofPattern("EEE MMM dd yyyy HH:mm:ss 'GMT+0000 (Coordinated Universal Time)'", java.util.Locale.US))
    }

    private fun edgeSsmlTimestamp(): String {
        return java.time.ZonedDateTime.now(java.time.ZoneOffset.UTC)
            .format(java.time.format.DateTimeFormatter.ofPattern("yyyy-MM-dd'T'HH:mm:ss.SSS", java.util.Locale.US))
    }

    private fun generateSecMsGec(): String {
        val winEpochOffset = 11644473600L
        val nowSeconds = System.currentTimeMillis() / 1000
        val adjustedSeconds = nowSeconds + winEpochOffset
        val roundedSeconds = adjustedSeconds - (adjustedSeconds % 300)
        val ticks = roundedSeconds * 10_000_000L
        val input = "$ticks$EDGE_TRUSTED_TOKEN"
        val digest = java.security.MessageDigest.getInstance("SHA-256").digest(input.toByteArray())
        return digest.joinToString("") { "%02X".format(it) }
    }

    private companion object {
        private const val EDGE_TRUSTED_TOKEN = "6A5AA1D4EAFF4E9FB37E23D68491D6F4"
        private const val EDGE_WS_ENDPOINT =
            "wss://speech.platform.bing.com/consumer/speech/synthesize/readaloud/edge/v1"
        private const val EDGE_ORIGIN = "chrome-extension://jdiccldimpdaibmpdkjnbmckianbfold"
        private const val CHROMIUM_FULL_VERSION = "143.0.3650.75"
        private const val SEC_MS_GEC_VERSION = "1-$CHROMIUM_FULL_VERSION"
        private const val CHUNK_BYTES = 24_000
    }
}

internal fun resolveEdgeVoice(request: TtsRequest, detectLanguage: () -> String): String {
    request.edgeVoiceOverride?.let { return it }
    val detectedLanguage = detectLanguage()
    return request.settingsSnapshot.edgeSettings.voiceConfigs.firstOrNull {
        it.languageCode.equals(detectedLanguage, ignoreCase = true)
    }?.voiceName ?: DEFAULT_EDGE_VOICE
}

internal class GoogleTranslateTtsProvider(
    private val httpClient: OkHttpClient,
    private val languageDetector: DeviceLanguageDetector,
    private val mp3Decoder: Mp3Decoder,
) {
    fun stream(
        request: TtsRequest,
        isStale: () -> Boolean,
        sink: LinkedBlockingDeque<ProviderAudioEvent>,
    ) {
        val languageCode = languageDetector.detectIso639_1(request.text)
        val encodedText = URLEncoder.encode(request.text, StandardCharsets.UTF_8.name())
        val url =
            "https://translate.google.com/translate_tts?ie=UTF-8&q=$encodedText&tl=$languageCode&client=tw-ob"

        val httpRequest = Request.Builder()
            .url(url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            )
            .build()

        runCatching {
            val call = httpClient.newCall(httpRequest)
            val handoff = TtsHttpResponseHandoff()
            val submitted = dispatchTtsPayload(request, isStale) {
                call.enqueue(handoff)
                true
            }
            if (submitted == null) return
            var received = false
            val mp3Bytes = try {
                handoff.await(isStale).also { received = it != null }
            } finally {
                handoff.abandon()
                if (!received) call.cancel()
            } ?: return
            if (isStale()) return
            val pcm = mp3Decoder.decodeToMonoPcm24k(mp3Bytes)
            if (isStale()) return
            check(pcm.isNotEmpty()) { "Google TTS audio decode failed." }
            chunkBytes(pcm, CHUNK_BYTES).forEach { chunk ->
                sink.offer(ProviderAudioEvent.PcmData(chunk))
            }
            sink.offer(ProviderAudioEvent.End)
        }.onFailure { error ->
            if (!isStale()) sink.offer(ProviderAudioEvent.Error(error.message ?: "Google Translate TTS failed."))
        }
    }

    private companion object {
        private const val CHUNK_BYTES = 24_000
    }
}

/** Owns response reading until cancellation or delivery of the complete body. */
internal class TtsHttpResponseHandoff : Callback {
    private val lock = Any()
    private val result = LinkedBlockingQueue<Result<ByteArray>>(1)
    private var abandoned = false

    override fun onFailure(call: Call, e: IOException) = deliver(Result.failure(e))

    override fun onResponse(call: Call, response: Response) {
        if (synchronized(lock) { abandoned }) {
            response.close()
            return
        }
        val value = runCatching {
            response.use {
                check(it.isSuccessful) { "Google TTS HTTP ${it.code}" }
                it.body.bytes()
            }
        }
        deliver(value)
    }

    private fun deliver(value: Result<ByteArray>) = synchronized(lock) {
        if (!abandoned) result.offer(value)
        Unit
    }

    fun await(isStale: () -> Boolean): ByteArray? {
        while (!isStale()) {
            val value = result.poll(50, TimeUnit.MILLISECONDS) ?: continue
            if (isStale()) return null
            return value.getOrThrow()
        }
        return null
    }

    fun abandon() = synchronized(lock) {
        abandoned = true
        result.clear()
    }
}
