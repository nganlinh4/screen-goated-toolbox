package dev.screengoated.toolbox.mobile.creation.worker

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
            "generate" -> when (runCatching { CreationGenerationMode.fromWireName(request.generationMode) }.getOrNull()) {
                CreationGenerationMode.FAST -> executionIndex == 0
                CreationGenerationMode.QUALITY -> executionIndex == 1
                null -> false
            }
            "segment", "refine" -> executionIndex == 1
            else -> false
        }
        CreationTool.IMAGE_TO_SVG -> request.operation == "generate"
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
    CreationTool.IMAGE_TO_3D -> when (runCatching { CreationGenerationMode.fromWireName(generationMode) }.getOrNull()) {
        CreationGenerationMode.FAST -> "3d-0"
        CreationGenerationMode.QUALITY -> "3d-1"
        null -> null
    }
    CreationTool.IMAGE_TO_SVG -> null
}
