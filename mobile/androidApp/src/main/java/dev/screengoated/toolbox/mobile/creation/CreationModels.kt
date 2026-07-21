package dev.screengoated.toolbox.mobile.creation

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject

internal enum class CreationTool(
    val wireName: String,
    val assetDirectory: String,
) {
    IMAGE_TO_3D("3d", "image-to-3d"),
    IMAGE_TO_SVG("svg", "image-to-svg");

    companion object {
        fun fromWireName(value: String?): CreationTool? = entries.firstOrNull {
            it.wireName == value
        }
    }
}

@Serializable
internal data class CreationJobStatus(
    val jobId: String? = null,
    val stage: String,
    val progressText: String,
    val phase: String? = null,
    val workspaceState: String? = null,
    val elapsedMs: Long? = null,
    val estimatedTotalMs: Long? = null,
    val progressRatio: Double? = null,
    val timingSampleCount: Long? = null,
    val outputPath: String? = null,
    val outputName: String? = null,
    val previewPath: String? = null,
    val sourceImagePath: String? = null,
    val isSegmented: Boolean = false,
    val canSegment: Boolean = false,
    val error: String? = null,
    val runtimeStatus: String = "installed",
    val model: String? = null,
    val creditsRemaining: Long? = null,
)

@Serializable
internal data class CreationHistoryEntry(
    val id: String,
    val tool: String,
    val sourcePath: String,
    val outputPath: String,
    val outputName: String,
    val createdAtMs: Long,
    val metadata: JsonObject = JsonObject(emptyMap()),
)

@Serializable
internal data class CreationWorkerRequest(
    val jobId: String,
    val tool: String,
    val operation: String,
    val imagePath: String,
    val outputPath: String,
    val outputName: String,
    val polycount: Int = CreationContract.DEFAULT_POLYCOUNT,
    val autoSegment: Boolean = false,
    val model: String = "simple",
    val taskId: String? = null,
    val previousOutputPath: String? = null,
)

@Serializable
internal data class CreationWorkerEvent(
    val jobId: String? = null,
    val event: String,
    val stage: String? = null,
    val progressText: String? = null,
    val progressKey: String? = null,
    val phase: String? = null,
    val progressRatio: Double? = null,
    val estimatedTotalMs: Long? = null,
    val taskId: String? = null,
    val outputPath: String? = null,
    val outputName: String? = null,
    val isSegmented: Boolean? = null,
    val canSegment: Boolean? = null,
    val creditsRemaining: Long? = null,
    val faces: Long? = null,
    val vertices: Long? = null,
    val error: String? = null,
    val ready: Boolean? = null,
)
