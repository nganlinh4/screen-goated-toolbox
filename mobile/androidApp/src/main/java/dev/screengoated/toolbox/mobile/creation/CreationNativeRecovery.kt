package dev.screengoated.toolbox.mobile.creation

import java.io.File
import java.util.UUID

internal fun recoverCreationNativeItem(tool: CreationTool, status: CreationJobStatus, supportsInstruction: (String) -> Boolean): CreationNativeItem {
    val references = if (tool == CreationTool.IMAGE_CREATOR) CreationImageSessions.statusReferences(status)
        else listOf(requireNotNull(status.sourceImagePath))
    val path = references.firstOrNull().orEmpty()
    val mode = CreationGenerationMode.fromWireName(status.generationMode)
    val autoSegment = status.autoSegment ?: false
    return CreationNativeItem(
        id = status.jobId ?: "recovered_${UUID.randomUUID()}",
        batchId = "recovered_${status.jobId}", sourcePath = path,
        sourceName = path.takeIf(String::isNotBlank)?.let(::File)?.name.orEmpty(),
        referencePaths = references, generationMode = mode.wireName,
        topology = CreationContract.initialTopology(mode, autoSegment, status.topology),
        polycount = status.polycount ?: CreationContract.DEFAULT_POLYCOUNT,
        model = status.model ?: "simple", backgroundMode = normalizeSvgBackgroundMode(status.backgroundMode),
        prompt = status.prompt.orEmpty(), instruction = status.instruction.orEmpty(),
        allowsInstruction = supportsInstruction(mode.wireName), autoSegment = autoSegment,
        segmentationLevel = status.segmentationLevel ?: "detailed", submitted = true,
        stage = status.toNativeStage(), status = status,
    )
}
