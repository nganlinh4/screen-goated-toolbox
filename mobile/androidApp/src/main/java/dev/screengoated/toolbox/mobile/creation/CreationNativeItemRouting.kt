package dev.screengoated.toolbox.mobile.creation

import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal fun routeCreationNativeItem(
    tool: CreationTool,
    item: CreationNativeItem,
    supportsInstruction: (String) -> Boolean,
): CreationNativeItem {
    if (tool != CreationTool.IMAGE_TO_3D) return item
    if (!item.status?.jobId.isNullOrBlank()) return item
    val route = CreationContract.route3dMode(
        CreationGenerationMode.fromWireName(item.generationMode),
        item.polycount,
        item.autoSegment,
    )
    val allowed = supportsInstruction(route.mode.wireName)
    return item.copy(
        generationMode = route.mode.wireName,
        polycount = route.polycount,
        autoSegment = route.autoSegment,
        topology = CreationContract.initialTopology(route.mode, route.autoSegment, item.topology),
        allowsInstruction = allowed,
        instruction = item.instruction.takeIf { allowed }.orEmpty(),
    )
}

internal fun applyCreationRuntimeCapabilities(
    tool: CreationTool,
    item: CreationNativeItem,
    supportsInstruction: (String) -> Boolean,
): CreationNativeItem {
    if (tool != CreationTool.IMAGE_TO_3D) return item
    if (!item.status?.jobId.isNullOrBlank()) {
        return item.copy(allowsInstruction = item.instruction.isNotBlank() || supportsInstruction(item.generationMode))
    }
    val allowed = supportsInstruction(item.generationMode)
    return if (item.allowsInstruction == allowed) {
        item
    } else {
        item.copy(
            allowsInstruction = allowed,
            instruction = item.instruction.takeIf { allowed }.orEmpty(),
        )
    }
}

/** Answers each mode once; the underlying read parses the runtime manifest. */
internal fun creationMemoizedInstructionSupport(
    supportsInstruction: (String) -> Boolean,
): (String) -> Boolean {
    val answers = java.util.concurrent.ConcurrentHashMap<String, Boolean>()
    return { mode -> answers.getOrPut(mode) { supportsInstruction(mode) } }
}

internal fun creationSubmissionArgs(
    tool: CreationTool,
    item: CreationNativeItem,
) = buildJsonObject {
    put("imagePath", item.sourcePath)
    put("generationMode", item.generationMode)
    if (tool == CreationTool.IMAGE_TO_3D) put("topology", item.topology)
    put("polycount", item.polycount)
    put("autoSegment", item.autoSegment)
    put("segmentationLevel", item.segmentationLevel)
    put("segmentationMode", if (item.autoSegment) "parts" else "none")
    put("model", item.model)
    put("backgroundMode", item.backgroundMode)
    item.instruction
        .takeIf { item.allowsInstruction && it.isNotBlank() }
        ?.let { put("instruction", it.trim()) }
}
