package dev.screengoated.toolbox.mobile.creation

import dev.screengoated.toolbox.mobile.creation.worker.creationRequiredWorkerKeyForGeneration

internal class CreationSurfaceWorkerCoordinator(
    private val workers: CreationWorkerPool,
) {
    fun acquire(tool: CreationTool, ownerId: String): String = "preparing".also {
        workers.acquireSurface(
            tool,
            "surface:$ownerId",
            creationRequiredWorkerKeyForGeneration(
                tool,
                CreationGenerationMode.QUALITY.wireName,
            ),
        )
    }

    fun release(tool: CreationTool, ownerId: String) = workers.release(tool, "surface:$ownerId")

    fun preparationStatus(tool: CreationTool): String = workers.preparationStatus(tool)

    fun supportsOptionalInstruction(mode: String): Boolean =
        workers.supportsOptionalInstruction(mode)

    fun removeRuntime() = workers.removeRuntime()
}
