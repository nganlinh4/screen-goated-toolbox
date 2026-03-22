package dev.screengoated.toolbox.mobile.history

import dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock

interface PresetHistoryRecorder {
    fun recordTextResult(
        block: ProcessingBlock,
        sourceText: String,
        resultText: String,
    )

    fun recordImageResult(
        block: ProcessingBlock,
        imageBytes: ByteArray,
        resultText: String,
    )

    fun recordAudioResult(
        block: ProcessingBlock,
        wavBytes: ByteArray,
        resultText: String,
    )
}

object NoOpPresetHistoryRecorder : PresetHistoryRecorder {
    override fun recordTextResult(
        block: ProcessingBlock,
        sourceText: String,
        resultText: String,
    ) = Unit

    override fun recordImageResult(
        block: ProcessingBlock,
        imageBytes: ByteArray,
        resultText: String,
    ) = Unit

    override fun recordAudioResult(
        block: ProcessingBlock,
        wavBytes: ByteArray,
        resultText: String,
    ) = Unit
}

class HistoryBackedPresetHistoryRecorder(
    private val historyRepository: HistoryRepository,
) : PresetHistoryRecorder {
    override fun recordTextResult(
        block: ProcessingBlock,
        sourceText: String,
        resultText: String,
    ) {
        if (!block.showOverlay || resultText.isBlank()) {
            return
        }
        historyRepository.saveText(
            resultText = resultText,
            inputText = sourceText,
        )
    }

    override fun recordImageResult(
        block: ProcessingBlock,
        imageBytes: ByteArray,
        resultText: String,
    ) {
        if (!block.showOverlay || resultText.isBlank()) {
            return
        }
        historyRepository.saveImage(
            pngBytes = imageBytes,
            resultText = resultText,
        )
    }

    override fun recordAudioResult(
        block: ProcessingBlock,
        wavBytes: ByteArray,
        resultText: String,
    ) {
        if (!block.showOverlay || resultText.isBlank() || wavBytes.isEmpty()) {
            return
        }
        historyRepository.saveAudio(
            wavBytes = wavBytes,
            resultText = resultText,
        )
    }
}
