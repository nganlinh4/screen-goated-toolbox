package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.preset.Preset
import dev.screengoated.toolbox.mobile.shared.preset.PresetHotkey
import dev.screengoated.toolbox.mobile.shared.preset.PresetType
import dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock
import dev.screengoated.toolbox.mobile.shared.preset.WindowGeometry
import kotlinx.serialization.Serializable

@Serializable
data class StoredPresetOverrides(
    val version: Int = 1,
    val builtInOverrides: Map<String, PresetOverride> = emptyMap(),
)

@Serializable
data class PresetOverride(
    val nameEn: String? = null,
    val nameVi: String? = null,
    val nameKo: String? = null,
    val presetType: PresetType? = null,
    val blocks: List<ProcessingBlock>? = null,
    val blockConnections: List<Pair<Int, Int>>? = null,
    val promptMode: String? = null,
    val textInputMode: String? = null,
    val audioSource: String? = null,
    val audioProcessingMode: String? = null,
    val realtimeWindowMode: String? = null,
    val videoCaptureMethod: String? = null,
    val autoPaste: Boolean? = null,
    val autoPasteNewline: Boolean? = null,
    val hideRecordingUi: Boolean? = null,
    val continuousInput: Boolean? = null,
    val autoStopRecording: Boolean? = null,
    val hotkeys: List<PresetHotkey>? = null,
    val isMaster: Boolean? = null,
    val showControllerUi: Boolean? = null,
    val isFavorite: Boolean? = null,
    val isUpcoming: Boolean? = null,
    val windowGeometry: WindowGeometry? = null,
) {
    fun isEmpty(): Boolean {
        return nameEn == null &&
            nameVi == null &&
            nameKo == null &&
            presetType == null &&
            blocks == null &&
            blockConnections == null &&
            promptMode == null &&
            textInputMode == null &&
            audioSource == null &&
            audioProcessingMode == null &&
            realtimeWindowMode == null &&
            videoCaptureMethod == null &&
            autoPaste == null &&
            autoPasteNewline == null &&
            hideRecordingUi == null &&
            continuousInput == null &&
            autoStopRecording == null &&
            hotkeys == null &&
            isMaster == null &&
            showControllerUi == null &&
            isFavorite == null &&
            isUpcoming == null &&
            windowGeometry == null
    }
}

fun Preset.applyOverride(override: PresetOverride): Preset {
    return copy(
        nameEn = override.nameEn ?: nameEn,
        nameVi = override.nameVi ?: nameVi,
        nameKo = override.nameKo ?: nameKo,
        presetType = override.presetType ?: presetType,
        blocks = override.blocks ?: blocks,
        blockConnections = override.blockConnections ?: blockConnections,
        promptMode = override.promptMode ?: promptMode,
        textInputMode = override.textInputMode ?: textInputMode,
        audioSource = override.audioSource ?: audioSource,
        audioProcessingMode = override.audioProcessingMode ?: audioProcessingMode,
        realtimeWindowMode = override.realtimeWindowMode ?: realtimeWindowMode,
        videoCaptureMethod = override.videoCaptureMethod ?: videoCaptureMethod,
        autoPaste = override.autoPaste ?: autoPaste,
        autoPasteNewline = override.autoPasteNewline ?: autoPasteNewline,
        hideRecordingUi = override.hideRecordingUi ?: hideRecordingUi,
        continuousInput = override.continuousInput ?: continuousInput,
        autoStopRecording = override.autoStopRecording ?: autoStopRecording,
        hotkeys = override.hotkeys ?: hotkeys,
        isMaster = override.isMaster ?: isMaster,
        showControllerUi = override.showControllerUi ?: showControllerUi,
        isFavorite = override.isFavorite ?: isFavorite,
        isUpcoming = override.isUpcoming ?: isUpcoming,
        windowGeometry = override.windowGeometry ?: windowGeometry,
    )
}

fun Preset.toOverrideComparedTo(canonical: Preset): PresetOverride {
    return PresetOverride(
        nameEn = nameEn.takeIf { it != canonical.nameEn },
        nameVi = nameVi.takeIf { it != canonical.nameVi },
        nameKo = nameKo.takeIf { it != canonical.nameKo },
        presetType = presetType.takeIf { it != canonical.presetType },
        blocks = blocks.takeIf { it != canonical.blocks },
        blockConnections = blockConnections.takeIf { it != canonical.blockConnections },
        promptMode = promptMode.takeIf { it != canonical.promptMode },
        textInputMode = textInputMode.takeIf { it != canonical.textInputMode },
        audioSource = audioSource.takeIf { it != canonical.audioSource },
        audioProcessingMode = audioProcessingMode.takeIf { it != canonical.audioProcessingMode },
        realtimeWindowMode = realtimeWindowMode.takeIf { it != canonical.realtimeWindowMode },
        videoCaptureMethod = videoCaptureMethod.takeIf { it != canonical.videoCaptureMethod },
        autoPaste = autoPaste.takeIf { it != canonical.autoPaste },
        autoPasteNewline = autoPasteNewline.takeIf { it != canonical.autoPasteNewline },
        hideRecordingUi = hideRecordingUi.takeIf { it != canonical.hideRecordingUi },
        continuousInput = continuousInput.takeIf { it != canonical.continuousInput },
        autoStopRecording = autoStopRecording.takeIf { it != canonical.autoStopRecording },
        hotkeys = hotkeys.takeIf { it != canonical.hotkeys },
        isMaster = isMaster.takeIf { it != canonical.isMaster },
        showControllerUi = showControllerUi.takeIf { it != canonical.showControllerUi },
        isFavorite = isFavorite.takeIf { it != canonical.isFavorite },
        isUpcoming = isUpcoming.takeIf { it != canonical.isUpcoming },
        windowGeometry = windowGeometry.takeIf { it != canonical.windowGeometry },
    )
}
