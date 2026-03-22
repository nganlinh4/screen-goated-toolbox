package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.preset.BlockType
import dev.screengoated.toolbox.mobile.shared.preset.Preset
import dev.screengoated.toolbox.mobile.shared.preset.PresetType

internal class PresetExecutionCapabilityResolver {
    fun resolveExecutionCapability(preset: Preset): PresetExecutionCapability {
        if (preset.isMaster || preset.showControllerUi) {
            return PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.CONTROLLER_MODE_NOT_READY,
            )
        }

        return when (preset.presetType) {
            PresetType.IMAGE -> resolveImageCapability(preset)
            PresetType.TEXT_SELECT -> resolveTextSelectCapability(preset)
            PresetType.TEXT_INPUT -> resolveTextInputCapability(preset)
            PresetType.MIC,
            PresetType.DEVICE_AUDIO,
            -> resolveAudioCapability(preset)
        }
    }

    fun resolvePlaceholderReasons(
        preset: Preset,
        executionCapability: PresetExecutionCapability,
    ): Set<PresetPlaceholderReason> {
        val reasons = linkedSetOf<PresetPlaceholderReason>()

        executionCapability.reason?.let(reasons::add)

        // auto_paste on Android = auto-copy to clipboard (user pastes manually)
        // No longer blocked
        if (preset.hotkeys.isNotEmpty()) {
            reasons += PresetPlaceholderReason.HOTKEYS_NOT_READY
        }
        return reasons
    }

    private fun resolveTextSelectCapability(preset: Preset): PresetExecutionCapability {
        // TEXT_SELECT presets can work with just an input adapter (e.g., read_aloud)
        // or with text processing blocks. More permissive than TEXT_INPUT.
        if (preset.blocks.isEmpty()) {
            return PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.TEXT_INPUT_OVERLAY_NOT_READY,
            )
        }

        // Check for unsupported text models
        val unsupportedTextModel = preset.blocks.firstOrNull { block ->
            block.blockType == BlockType.TEXT && !isTextModelSupported(block.model)
        }
        if (unsupportedTextModel != null) {
            return PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.MODEL_PROVIDER_NOT_READY,
            )
        }

        // Block non-text block types (image/audio blocks not ready on Android)
        if (preset.blocks.any { it.blockType !in setOf(BlockType.INPUT_ADAPTER, BlockType.TEXT) }) {
            return PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.NON_TEXT_GRAPH_NOT_READY,
            )
        }

        return PresetExecutionCapability(supported = true)
    }

    private fun resolveTextInputCapability(preset: Preset): PresetExecutionCapability {
        val hasTextBlocks = preset.blocks.any { it.blockType == BlockType.TEXT }
        val hasInputAdapterOverlay = preset.blocks.any {
            it.blockType == BlockType.INPUT_ADAPTER && it.showOverlay
        }
        if (!hasTextBlocks && !hasInputAdapterOverlay) {
            return PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.TEXT_INPUT_OVERLAY_NOT_READY,
            )
        }

        val unsupportedTextModel = preset.blocks.firstOrNull { block ->
            block.blockType == BlockType.TEXT && !isTextModelSupported(block.model)
        }
        if (unsupportedTextModel != null) {
            return PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.MODEL_PROVIDER_NOT_READY,
            )
        }

        if (preset.blocks.any { it.blockType !in setOf(BlockType.INPUT_ADAPTER, BlockType.TEXT) }) {
            return PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.NON_TEXT_GRAPH_NOT_READY,
            )
        }

        return PresetExecutionCapability(supported = true)
    }

    private fun resolveImageCapability(preset: Preset): PresetExecutionCapability {
        if (preset.blocks.isEmpty()) {
            return PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.IMAGE_CAPTURE_NOT_READY,
            )
        }
        val unsupportedVisionModel = preset.blocks.firstOrNull { block ->
            block.blockType == BlockType.IMAGE && !isVisionModelSupported(block.model)
        }
        if (unsupportedVisionModel != null) {
            return PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.MODEL_PROVIDER_NOT_READY,
            )
        }
        val unsupportedTextModel = preset.blocks.firstOrNull { block ->
            block.blockType == BlockType.TEXT && !isTextModelSupported(block.model)
        }
        if (unsupportedTextModel != null) {
            return PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.MODEL_PROVIDER_NOT_READY,
            )
        }
        if (preset.blocks.any { it.blockType !in setOf(BlockType.INPUT_ADAPTER, BlockType.IMAGE, BlockType.TEXT) }) {
            return PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.NON_TEXT_GRAPH_NOT_READY,
            )
        }
        return PresetExecutionCapability(supported = true)
    }

    private fun resolveAudioCapability(preset: Preset): PresetExecutionCapability {
        if (preset.blocks.isEmpty()) {
            return PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.AUDIO_CAPTURE_NOT_READY,
            )
        }
        if (preset.audioProcessingMode == "realtime") {
            return resolveRealtimeAudioCapability(preset)
        }

        val unsupportedAudioModel = preset.blocks.firstOrNull { block ->
            block.blockType == BlockType.AUDIO && !isAudioModelSupported(block.model)
        }
        if (unsupportedAudioModel != null) {
            return PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.MODEL_PROVIDER_NOT_READY,
            )
        }
        val unsupportedTextModel = preset.blocks.firstOrNull { block ->
            block.blockType == BlockType.TEXT && !isTextModelSupported(block.model)
        }
        if (unsupportedTextModel != null) {
            return PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.MODEL_PROVIDER_NOT_READY,
            )
        }
        if (preset.blocks.any { it.blockType !in setOf(BlockType.INPUT_ADAPTER, BlockType.AUDIO, BlockType.TEXT) }) {
            return PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.NON_TEXT_GRAPH_NOT_READY,
            )
        }
        return PresetExecutionCapability(supported = true)
    }

    private fun resolveRealtimeAudioCapability(preset: Preset): PresetExecutionCapability {
        val realtimeBlocks = preset.blocks.filter { it.blockType != BlockType.INPUT_ADAPTER }
        if (realtimeBlocks.isEmpty() || realtimeBlocks.size > 2) {
            return PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.NON_TEXT_GRAPH_NOT_READY,
            )
        }
        val audioBlock = realtimeBlocks.firstOrNull()
        if (audioBlock?.blockType != BlockType.AUDIO || !isAudioModelSupported(audioBlock.model)) {
            return PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.REALTIME_AUDIO_NOT_READY,
            )
        }
        val translationBlock = realtimeBlocks.getOrNull(1)
        if (translationBlock != null && (translationBlock.blockType != BlockType.TEXT || !isTextModelSupported(translationBlock.model))) {
            return PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.MODEL_PROVIDER_NOT_READY,
            )
        }
        return PresetExecutionCapability(supported = true)
    }

    private fun isVisionModelSupported(modelId: String): Boolean {
        val descriptor = PresetModelCatalog.getById(modelId) ?: return false
        if (descriptor.modelType != PresetModelType.VISION) return false
        return descriptor.provider in setOf(
            PresetModelProvider.GOOGLE,
            PresetModelProvider.GROQ,
            PresetModelProvider.OPENROUTER,
            PresetModelProvider.OLLAMA,
            PresetModelProvider.QRSERVER,
        )
    }

    private fun isTextModelSupported(modelId: String): Boolean {
        val descriptor = PresetModelCatalog.getById(modelId) ?: return false
        if (descriptor.modelType != PresetModelType.TEXT) {
            return false
        }
        return descriptor.provider in setOf(
            PresetModelProvider.GOOGLE,
            PresetModelProvider.CEREBRAS,
            PresetModelProvider.GROQ,
            PresetModelProvider.OPENROUTER,
            PresetModelProvider.GOOGLE_GTX,
            PresetModelProvider.OLLAMA,
        )
    }

    private fun isAudioModelSupported(modelId: String): Boolean {
        val descriptor = PresetModelCatalog.getById(modelId) ?: return false
        if (descriptor.modelType != PresetModelType.AUDIO) {
            return false
        }
        return descriptor.provider in setOf(
            PresetModelProvider.GOOGLE,
            PresetModelProvider.GROQ,
            PresetModelProvider.GEMINI_LIVE,
            PresetModelProvider.PARAKEET,
        )
    }

}

internal fun PresetPlaceholderReason.message(): String = when (this) {
    PresetPlaceholderReason.IMAGE_CAPTURE_NOT_READY ->
        "Image preset capture is not ready on Android yet."
    PresetPlaceholderReason.TEXT_SELECTION_NOT_READY ->
        "Selected-text capture is not ready on Android yet."
    PresetPlaceholderReason.TEXT_INPUT_OVERLAY_NOT_READY ->
        "Overlay-style text input is not ready on Android yet."
    PresetPlaceholderReason.MODEL_PROVIDER_NOT_READY ->
        "This preset uses a text model/provider runtime that Android does not implement yet."
    PresetPlaceholderReason.AUDIO_CAPTURE_NOT_READY ->
        "This audio preset still needs a supported Android capture/runtime path."
    PresetPlaceholderReason.REALTIME_AUDIO_NOT_READY ->
        "This realtime audio preset shape is not supported on Android yet."
    PresetPlaceholderReason.HTML_RESULT_NOT_READY ->
        "HTML result overlays are not ready on Android yet."
    PresetPlaceholderReason.CONTROLLER_MODE_NOT_READY ->
        "Controller presets are not ready on Android yet."
    PresetPlaceholderReason.AUTO_PASTE_NOT_READY ->
        "Auto-paste integration is not ready on Android yet."
    PresetPlaceholderReason.HOTKEYS_NOT_READY ->
        "Hotkeys are not ready on Android yet."
    PresetPlaceholderReason.GRAPH_EDITING_NOT_READY ->
        "Use the edit button to open the full node graph editor."
    PresetPlaceholderReason.NON_TEXT_GRAPH_NOT_READY ->
        "This preset graph still uses Android-unsupported block types."
}
