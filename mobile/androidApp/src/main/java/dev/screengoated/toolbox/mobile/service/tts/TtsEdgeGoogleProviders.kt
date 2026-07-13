package dev.screengoated.toolbox.mobile.service.tts

import dev.screengoated.toolbox.mobile.service.websocket.BlockingWebSocketSession
import dev.screengoated.toolbox.mobile.service.websocket.WebSocketEvent
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONObject
import java.net.URLEncoder
import java.nio.charset.StandardCharsets
import java.util.UUID
import java.util.concurrent.LinkedBlockingDeque

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

        BlockingWebSocketSession(httpClient, socketRequest).use { session ->
            if (!session.awaitOpen(10_000)) {
                val failEvent = session.poll(0)
                val reason = if (failEvent is WebSocketEvent.Failure) failEvent.throwable.message else "timeout"
                sink.offer(ProviderAudioEvent.Error("Edge TTS websocket failed: $reason"))
                return
            }

            val configMessage =
                "X-Timestamp:${edgeHeaderTimestamp()}\r\n" +
                    "Content-Type:application/json; charset=utf-8\r\n" +
                    "Path:speech.config\r\n\r\n" +
                    "{\"context\":{\"synthesis\":{\"audio\":{\"metadataoptions\":{\"sentenceBoundaryEnabled\":\"false\",\"wordBoundaryEnabled\":\"false\"},\"outputFormat\":\"audio-24khz-48kbitrate-mono-mp3\"}}}}"

            if (!session.sendText(configMessage)) {

                sink.offer(ProviderAudioEvent.Error("Edge TTS config payload was rejected."))
                return
            }

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
            if (!session.sendText(ssmlMessage)) {

                sink.offer(ProviderAudioEvent.Error("Edge TTS SSML payload was rejected."))
                return
            }

            val mp3Data = ArrayList<Byte>()
            while (!isStale()) {
                when (val event = session.poll(250)) {
                    null -> Unit
                    is WebSocketEvent.Binary -> {
                        val bytes = event.payload.toByteArray()
                        if (bytes.size >= 2) {
                            val headerLength = ((bytes[0].toInt() and 0xFF) shl 8) or (bytes[1].toInt() and 0xFF)
                            val audioStart = 2 + headerLength
                            if (bytes.size > audioStart) {
                                val header = bytes.copyOfRange(2, audioStart)
                                if (header.toString(StandardCharsets.UTF_8).contains("Path:audio")) {
                                    bytes.copyOfRange(audioStart, bytes.size).forEach(mp3Data::add)
                                }
                            }
                        }
                    }

                    is WebSocketEvent.Text -> {

                        if (event.payload.contains("Path:turn.end")) {

                            emitDecodedMp3(mp3Data.toByteArray(), sink)
                            return
                        }
                    }

                    is WebSocketEvent.Failure -> {

                        sink.offer(ProviderAudioEvent.Error(event.throwable.message ?: "Edge TTS websocket failed."))
                        return
                    }

                    WebSocketEvent.Closed -> {

                        emitDecodedMp3(mp3Data.toByteArray(), sink)
                        return
                    }
                }
            }

        }
    }

    private fun resolveVoice(request: TtsRequest): String {
        val detectedLanguage = languageDetector.detectIso639_1(request.text)
        val configs = request.settingsSnapshot.edgeSettings.voiceConfigs
        val matched = configs.firstOrNull {
            it.languageCode.equals(detectedLanguage, ignoreCase = true)
        }
        val voice = matched?.voiceName ?: DEFAULT_EDGE_VOICE

        return voice
    }

    private fun emitDecodedMp3(
        mp3: ByteArray,
        sink: LinkedBlockingDeque<ProviderAudioEvent>,
    ) {

        val pcm = try {
            mp3Decoder.decodeToMonoPcm24k(mp3)
        } catch (e: Exception) {

            sink.offer(ProviderAudioEvent.Error("Edge TTS MP3 decode failed: ${e.message}"))
            return
        }

        if (pcm.isEmpty()) {
            sink.offer(ProviderAudioEvent.Error("Edge TTS did not return playable audio."))
            return
        }
        chunkBytes(pcm, CHUNK_BYTES).forEach { chunk ->
            sink.offer(ProviderAudioEvent.PcmData(chunk))
        }
        sink.offer(ProviderAudioEvent.End)
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
        private const val DEFAULT_EDGE_VOICE = "en-US-AriaNeural"
        private const val CHUNK_BYTES = 24_000
    }
}

internal class GoogleTranslateTtsProvider(
    private val httpClient: OkHttpClient,
    private val languageDetector: DeviceLanguageDetector,
    private val mp3Decoder: Mp3Decoder,
) {
    fun stream(
        request: TtsRequest,
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
            httpClient.newCall(httpRequest).execute().use { response ->
                check(response.isSuccessful) { "Google TTS HTTP ${response.code}" }
                val mp3Bytes = response.body.bytes()
                val pcm = mp3Decoder.decodeToMonoPcm24k(mp3Bytes)
                check(pcm.isNotEmpty()) { "Google TTS audio decode failed." }
                chunkBytes(pcm, CHUNK_BYTES).forEach { chunk ->
                    sink.offer(ProviderAudioEvent.PcmData(chunk))
                }
                sink.offer(ProviderAudioEvent.End)
            }
        }.onFailure { error ->
            sink.offer(ProviderAudioEvent.Error(error.message ?: "Google Translate TTS failed."))
        }
    }

    private companion object {
        private const val CHUNK_BYTES = 24_000
    }
}
