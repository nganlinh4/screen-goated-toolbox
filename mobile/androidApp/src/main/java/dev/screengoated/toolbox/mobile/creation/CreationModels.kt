package dev.screengoated.toolbox.mobile.creation

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject

internal enum class CreationTool(
    val wireName: String,
) {
    IMAGE_TO_3D("3d"),
    IMAGE_TO_SVG("svg"),
    IMAGE_CREATOR("image");

    companion object {
        fun fromWireName(value: String?): CreationTool? = entries.firstOrNull {
            it.wireName == value
        }
    }
}

@Serializable
internal data class CreationJobStatus(
    val jobId: String? = null,
    val dispatchId: String? = null,
    val operation: String? = null,
    val generationMode: String? = null,
    val polycount: Int? = null,
    val autoSegment: Boolean? = null,
    val stage: String,
    val progressText: String,
    val phase: String? = null,
    val elapsedMs: Long? = null,
    val estimatedTotalMs: Long? = null,
    val progressRatio: Double? = null,
    val timingSampleCount: Long? = null,
    val outputPath: String? = null,
    val outputName: String? = null,
    val previewPath: String? = null,
    val sourceImagePath: String? = null,
    val sourceImagePaths: List<String> = emptyList(),
    val prompt: String? = null,
    val instruction: String? = null,
    val mimeType: String? = null,
    val width: Int? = null,
    val height: Int? = null,
    val isSegmented: Boolean = false,
    val canSegment: Boolean = false,
    val error: String? = null,
    val runtimeStatus: String = "installed",
    val model: String? = null,
    val backgroundMode: String? = null,
    val faces: Long? = null,
    val vertices: Long? = null,
)

@Serializable
internal data class CreationHistoryEntry(
    val id: String,
    val dispatchId: String? = null,
    val tool: String,
    val sourcePath: String,
    val outputPath: String,
    val outputName: String,
    val createdAtMs: Long,
    val metadata: JsonObject = JsonObject(emptyMap()),
    val committedSize: Long? = null,
    val committedSha256: String? = null,
    val committedIdentity: String? = null,
)

@Serializable
internal data class CreationSourceDescriptor(
    val path: String,
    val sizeBytes: Long,
    val sha256: String,
)

@Serializable
internal data class CreationWorkerRequest(
    val jobId: String,
    val acceptedAtMs: Long = 0L,
    val deadlineAtMs: Long = 0L,
    val dispatchId: String = "",
    val requestFingerprint: String = "",
    val sourceDescriptors: List<CreationSourceDescriptor> = emptyList(),
    val tool: String,
    val generationMode: String? = null,
    val operation: String,
    val imagePath: String,
    val imagePaths: List<String> = emptyList(),
    val prompt: String? = null,
    val instruction: String? = null,
    val outputPath: String,
    val outputName: String,
    val polycount: Int = CreationContract.DEFAULT_POLYCOUNT,
    val autoSegment: Boolean = false,
    val model: String = "simple",
    val backgroundMode: String = "opaque",
    val continuationToken: String? = null,
    val previousOutputPath: String? = null,
)

@Serializable
internal data class CreationWorkerEvent(
    val jobId: String? = null,
    val generationMode: String? = null,
    val event: String,
    val stage: String? = null,
    val failureCode: String? = null,
    val progressRatio: Double? = null,
    val estimatedTotalMs: Long? = null,
    val timingSampleCount: Long? = null,
    val continuationToken: String? = null,
    val outputPath: String? = null,
    val outputName: String? = null,
    val mimeType: String? = null,
    val width: Int? = null,
    val height: Int? = null,
    val isSegmented: Boolean? = null,
    val canSegment: Boolean? = null,
    val faces: Long? = null,
    val vertices: Long? = null,
    val ready: Boolean? = null,
)
