package dev.screengoated.toolbox.mobile.creation

import java.io.File
import java.util.UUID

internal fun recoverCreationNativeItem(tool: CreationTool, status: CreationJobStatus, supportsInstruction: (String) -> Boolean): CreationNativeItem {
    val path = requireNotNull(status.sourceImagePath)
    val mode = CreationGenerationMode.fromWireName(status.generationMode)
    val autoSegment = status.autoSegment ?: false
    return CreationNativeItem(
        id = status.jobId ?: "recovered_${UUID.randomUUID()}",
        batchId = "recovered_${status.jobId}", sourcePath = path,
        sourceName = path.takeIf(String::isNotBlank)?.let(::File)?.name.orEmpty(),
        generationMode = mode.wireName,
        topology = CreationContract.initialTopology(mode, autoSegment, status.topology),
        polycount = status.polycount ?: CreationContract.DEFAULT_POLYCOUNT,
        model = status.model ?: "simple", backgroundMode = normalizeSvgBackgroundMode(status.backgroundMode),
        allowsInstruction = supportsInstruction(mode.wireName), autoSegment = autoSegment,
        segmentationLevel = status.segmentationLevel ?: "detailed", submitted = true,
        stage = status.toNativeStage(), status = status,
    )
}
