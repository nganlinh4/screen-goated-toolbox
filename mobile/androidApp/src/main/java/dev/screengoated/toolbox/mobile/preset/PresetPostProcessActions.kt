package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock

/**
 * Per-block post-processing actions — called by the executor after each block
 * completes, REGARDLESS of showOverlay. Matches Windows step.rs post-processing.
 */
interface PresetPostProcessActions {
    /** Copy result text to clipboard. Called if block.autoCopy is true. */
    fun handleAutoCopy(block: ProcessingBlock, resultText: String)

    /** Speak result text via TTS. Called if block.autoSpeak is true. */
    fun handleAutoSpeak(block: ProcessingBlock, resultText: String, blockIdx: Int)

    /** Paste clipboard into source app. Called after all blocks if preset.autoPaste is true. */
    fun handleAutoPaste()
}

/** No-op implementation for contexts without TTS/clipboard (e.g., tests). */
object NoOpPostProcessActions : PresetPostProcessActions {
    override fun handleAutoCopy(block: ProcessingBlock, resultText: String) {}
    override fun handleAutoSpeak(block: ProcessingBlock, resultText: String, blockIdx: Int) {}
    override fun handleAutoPaste() {}
}
