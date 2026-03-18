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
            PresetType.IMAGE -> PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.IMAGE_CAPTURE_NOT_READY,
            )
            PresetType.TEXT_SELECT -> PresetExecutionCapability(
                supported = false,
                reason = PresetPlaceholderReason.TEXT_SELECTION_NOT_READY,
            )
            PresetType.TEXT_INPUT -> resolveTextInputCapability(preset)
            PresetType.MIC,
            PresetType.DEVICE_AUDIO,
            -> PresetExecutionCapability(
                supported = false,
                reason = if (preset.audioProcessingMode == "realtime") {
                    PresetPlaceholderReason.REALTIME_AUDIO_NOT_READY
                } else {
                    PresetPlaceholderReason.AUDIO_CAPTURE_NOT_READY
                },
            )
        }
    }

    fun resolvePlaceholderReasons(
        preset: Preset,
        executionCapability: PresetExecutionCapability,
    ): Set<PresetPlaceholderReason> {
        val reasons = linkedSetOf<PresetPlaceholderReason>()

        executionCapability.reason?.let(reasons::add)

        if (preset.autoPaste || preset.autoPasteNewline) {
            reasons += PresetPlaceholderReason.AUTO_PASTE_NOT_READY
        }
        if (preset.hotkeys.isNotEmpty()) {
            reasons += PresetPlaceholderReason.HOTKEYS_NOT_READY
        }
        return reasons
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
        "Audio capture presets are not ready on Android yet."
    PresetPlaceholderReason.REALTIME_AUDIO_NOT_READY ->
        "Realtime audio presets are not ready on Android yet."
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
        "Only text-input preset graphs are executable on Android right now."
}
