package dev.screengoated.toolbox.mobile.shared.preset

import kotlinx.serialization.Serializable

@Serializable
enum class PresetType { IMAGE, TEXT_SELECT, TEXT_INPUT, MIC, DEVICE_AUDIO }

@Serializable
enum class BlockType { INPUT_ADAPTER, IMAGE, TEXT, AUDIO }

@Serializable
data class ProcessingBlock(
    val id: String,
    val blockType: BlockType,
    val model: String,
    val prompt: String = "",
    val languageVars: Map<String, String> = emptyMap(),
    val streamingEnabled: Boolean = true,
    val renderMode: String = "markdown_stream",
    val showOverlay: Boolean = true,
    val autoCopy: Boolean = false,
    val autoSpeak: Boolean = false,
)

@Serializable
data class Preset(
    val id: String,
    val nameEn: String,
    val nameVi: String,
    val nameKo: String,
    val presetType: PresetType,
    val blocks: List<ProcessingBlock>,
    val blockConnections: List<Pair<Int, Int>> = emptyList(),
    val promptMode: String = "fixed",
    val textInputMode: String = "type",
    val audioSource: String = "mic",
    val autoPaste: Boolean = false,
    val continuousInput: Boolean = false,
    val autoStopRecording: Boolean = false,
    val isMaster: Boolean = false,
    val isUpcoming: Boolean = false,
) {
    fun name(lang: String): String = when (lang) {
        "vi" -> nameVi
        "ko" -> nameKo
        else -> nameEn
    }
}

sealed class PresetInput {
    data class Text(val text: String) : PresetInput()
    data class Image(val pngBytes: ByteArray) : PresetInput()
    data class Audio(val wavBytes: ByteArray) : PresetInput()
}

data class BlockResult(
    val blockIdx: Int,
    val text: String,
    val model: String,
)

fun textBlock(
    model: String,
    prompt: String,
    vararg langVars: Pair<String, String>,
) = ProcessingBlock(
    id = "text_${model.replace("/", "_")}",
    blockType = BlockType.TEXT,
    model = model,
    prompt = prompt,
    languageVars = langVars.toMap(),
)

fun imageBlock(
    model: String,
    prompt: String,
    vararg langVars: Pair<String, String>,
) = ProcessingBlock(
    id = "image_${model.replace("/", "_")}",
    blockType = BlockType.IMAGE,
    model = model,
    prompt = prompt,
    languageVars = langVars.toMap(),
)

fun audioBlock(
    model: String,
    prompt: String = "",
    vararg langVars: Pair<String, String>,
) = ProcessingBlock(
    id = "audio_${model.replace("/", "_")}",
    blockType = BlockType.AUDIO,
    model = model,
    prompt = prompt,
    languageVars = langVars.toMap(),
)

fun inputAdapter() = ProcessingBlock(
    id = "input",
    blockType = BlockType.INPUT_ADAPTER,
    model = "",
)
