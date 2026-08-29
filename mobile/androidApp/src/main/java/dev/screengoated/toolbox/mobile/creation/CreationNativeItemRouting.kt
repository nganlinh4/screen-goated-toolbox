package dev.screengoated.toolbox.mobile.creation

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal fun routeCreationNativeItem(
    tool: CreationTool,
    item: CreationNativeItem,
    supportsInstruction: (String) -> Boolean,
): CreationNativeItem {
    if (tool != CreationTool.IMAGE_TO_3D) return item
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

internal fun creationSubmissionArgs(
    tool: CreationTool,
    item: CreationNativeItem,
) = buildJsonObject {
    if (tool == CreationTool.IMAGE_CREATOR) {
        if (item.referencePaths.isNotEmpty()) {
            put("imagePaths", JsonArray(item.referencePaths.map(::JsonPrimitive)))
        }
    } else {
        put("imagePath", item.sourcePath)
    }
    put("generationMode", item.generationMode)
    put("polycount", item.polycount)
    put("autoSegment", item.autoSegment)
    put("segmentationMode", if (item.autoSegment) "parts" else "none")
    put("model", item.model)
    put("backgroundMode", item.backgroundMode)
    item.instruction
        .takeIf { item.allowsInstruction && it.isNotBlank() }
        ?.let { put("instruction", it.trim()) }
    item.prompt.takeIf { tool == CreationTool.IMAGE_CREATOR }?.let {
        put("prompt", it.trim())
    }
}
