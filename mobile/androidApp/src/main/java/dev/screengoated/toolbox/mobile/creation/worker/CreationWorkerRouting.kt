package dev.screengoated.toolbox.mobile.creation.worker

import dev.screengoated.toolbox.mobile.creation.CreationContract
import dev.screengoated.toolbox.mobile.creation.CreationGenerationMode
import dev.screengoated.toolbox.mobile.creation.CreationTool
import dev.screengoated.toolbox.mobile.creation.CreationWorkerRequest

internal fun creationWorkerStructurallySupports(
    tool: CreationTool,
    executionIndex: Int,
    request: CreationWorkerRequest,
): Boolean {
    if (request.tool != tool.wireName || executionIndex !in 0..1) return false
    return when (tool) {
        CreationTool.IMAGE_TO_3D -> when (request.operation) {
            "generate" -> when (CreationGenerationMode.fromWireName(request.generationMode)) {
                CreationGenerationMode.FAST -> executionIndex == 0
                CreationGenerationMode.QUALITY -> executionIndex == 1
            }
            "segment", "refine" -> executionIndex == 1
            else -> false
        }
        CreationTool.IMAGE_TO_SVG -> request.operation == "generate"
        CreationTool.IMAGE_CREATOR ->
            request.operation == CreationContract.IMAGE_CREATOR_OPERATION
    }
}

internal fun creationRequiredWorkerKey(
    tool: CreationTool,
    request: CreationWorkerRequest,
): String? = (0..1)
    .filter { creationWorkerStructurallySupports(tool, it, request) }
    .singleOrNull()
    ?.let { "${tool.wireName}-$it" }

internal fun creationRequiredWorkerKeyForGeneration(
    tool: CreationTool,
    generationMode: String,
): String? = when (tool) {
    CreationTool.IMAGE_TO_3D -> when (CreationGenerationMode.fromWireName(generationMode)) {
        CreationGenerationMode.FAST -> "3d-0"
        CreationGenerationMode.QUALITY -> "3d-1"
    }
    CreationTool.IMAGE_TO_SVG, CreationTool.IMAGE_CREATOR -> null
}
