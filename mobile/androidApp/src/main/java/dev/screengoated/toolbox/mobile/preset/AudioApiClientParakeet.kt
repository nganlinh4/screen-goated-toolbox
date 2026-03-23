package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.service.parakeet.ParakeetEngine
import java.io.File
import java.io.IOException

internal suspend fun AudioApiClient.transcribeWithParakeet(
    model: PresetModelDescriptor,
    wavBytes: ByteArray,
    uiLanguage: String,
    onChunk: (String) -> Unit,
): String {
    val session = openParakeetStreamingSession(
        _model = model,
        _uiLanguage = uiLanguage,
        onChunk = onChunk,
    )
    val samples = PresetAudioCodec.decodePcm16MonoWav(wavBytes)
    return try {
        val chunkSize = 2_560
        var offset = 0
        while (offset < samples.size) {
            val end = (offset + chunkSize).coerceAtMost(samples.size)
            session.appendPcm16Chunk(samples.copyOfRange(offset, end))
            offset = end
        }
        session.finish().transcript
    } finally {
        session.cancel()
    }
}

internal fun AudioApiClient.openParakeetStreamingSession(
    _model: PresetModelDescriptor,
    _uiLanguage: String,
    onChunk: (String) -> Unit,
): AudioStreamingSession {
    if (!parakeetModelManager.isInstalled()) {
        throw IOException("PARAKEET_MODEL_NOT_INSTALLED")
    }
    val modelDir = File(appContext.filesDir, "models/parakeet")
    val engine = ParakeetEngine(modelDir, parakeetModelManager.ortLibDir())
    val accumulator = StringBuilder()
    return object : AudioStreamingSession {
        override suspend fun appendPcm16Chunk(chunk: ShortArray) {
            val floatChunk = FloatArray(chunk.size) { index -> chunk[index] / 32768f }
            val text = processSentencePieceText(engine.transcribe(floatChunk))
            if (text.isNotBlank()) {
                accumulator.append(text)
                onChunk(text)
            }
        }

        override suspend fun finish(): AudioStreamingTranscriptResult {
            repeat(3) {
                val text = processSentencePieceText(engine.transcribe(FloatArray(2_560)))
                if (text.isNotBlank()) {
                    accumulator.append(text)
                    onChunk(text)
                }
            }
            return AudioStreamingTranscriptResult(
                transcript = accumulator.toString(),
                producedRealtimePaste = false,
            )
        }

        override fun cancel() {
            engine.close()
        }
    }
}

private fun processSentencePieceText(rawText: String): String {
    return rawText
        .replace('▁', ' ')
        .replace(Regex("\\s+"), " ")
}
