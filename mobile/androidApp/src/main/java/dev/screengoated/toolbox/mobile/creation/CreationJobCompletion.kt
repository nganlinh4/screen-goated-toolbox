package dev.screengoated.toolbox.mobile.creation

import java.io.File
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal data class CreationContinuation(
    val ownerId: String,
    val engineId: String,
    val token: String,
    val sourcePath: String,
    val outputPath: String,
    val outputName: String,
    val createdAtMs: Long,
)

internal data class FinishedCreation(
    val request: CreationWorkerRequest,
    val status: CreationJobStatus,
    val segmented: Boolean,
    val publishedPath: String,
    val continuation: CreationContinuation?,
)

internal data class PreparedCreation(
    val engineId: String,
    val ownerId: String,
    val request: CreationWorkerRequest,
    val current: CreationJobStatus,
    val event: CreationWorkerEvent,
    val stagingPath: String,
    val mimeType: String,
    val imageDimensions: CreationImageDimensions?,
    val segmented: Boolean,
    val canSegment: Boolean,
    val faces: Long?,
    val vertices: Long?,
)

internal class CreationJobFinisher(
    private val files: CreationFileStore,
    private val history: CreationHistoryStore,
) {
    fun prepare(
        engineId: String,
        ownerId: String,
        request: CreationWorkerRequest,
        current: CreationJobStatus,
        event: CreationWorkerEvent,
    ): PreparedCreation {
        val tool = requireNotNull(CreationTool.fromWireName(request.tool))
        require(
            event.outputPath == null ||
                File(event.outputPath).absoluteFile.normalize() ==
                File(request.outputPath).absoluteFile.normalize(),
        ) { "Creation returned a conflicting output path" }
        val staging = files.sealReservedStagingFile(tool, request.outputPath)
        return try {
            require(staging.length() > 0L) { "Creation ended without an output file" }
            var imageDimensions: CreationImageDimensions? = null
            val mime = when (tool) {
                CreationTool.IMAGE_TO_3D -> {
                    CreationArtifactValidator.validateGlb(staging)
                    "model/gltf-binary"
                }
                CreationTool.IMAGE_TO_SVG -> {
                    CreationArtifactValidator.validateSvg(staging)
                    "image/svg+xml"
                }
                CreationTool.IMAGE_CREATOR -> {
                    require(event.mimeType == null || event.mimeType == "image/png") {
                        "The image result is not a PNG image"
                    }
                    imageDimensions = CreationArtifactValidator.validatePng(
                        staging,
                        event.width,
                        event.height,
                    )
                    "image/png"
                }
            }
            val segmented = validatedCreationSegmentation(request, event)
            val canSegment = tool == CreationTool.IMAGE_TO_3D &&
                request.operation == "generate" &&
                request.generationMode == CreationGenerationMode.QUALITY.wireName &&
                !request.autoSegment &&
                CreationContract.canContinueSegmentation(segmented, event.canSegment == true) &&
                !event.continuationToken.isNullOrBlank()
            PreparedCreation(
                engineId,
                ownerId,
                request,
                current,
                event,
                staging.absolutePath,
                mime,
                imageDimensions,
                segmented,
                canSegment,
                event.faces.boundedCreationGeometryCount(tool),
                event.vertices.boundedCreationGeometryCount(tool),
            )
        } catch (failure: Throwable) {
            files.deleteManagedPath(staging.absolutePath)
            throw failure
        }
    }

    fun completePublished(
        prepared: PreparedCreation,
        published: String,
        outputName: String,
    ): FinishedCreation {
        val request = prepared.request
        val event = prepared.event
        val segmented = prepared.segmented
        val canSegment = prepared.canSegment
        val imageDimensions = prepared.imageDimensions
        val mime = prepared.mimeType
        val current = prepared.current
        val status = current.copy(
            stage = "done",
            progressText = when (request.tool) {
                CreationTool.IMAGE_TO_3D.wireName -> "Model ready."
                CreationTool.IMAGE_TO_SVG.wireName -> "Vector ready"
                else -> "Image ready"
            },
            phase = "complete",
            progressRatio = 1.0,
            outputPath = published,
            outputName = outputName,
            mimeType = mime,
            width = imageDimensions?.width,
            height = imageDimensions?.height,
            generationMode = request.generationMode,
            isSegmented = segmented,
            canSegment = canSegment,
            faces = prepared.faces,
            vertices = prepared.vertices,
            error = null,
        )
        val continuation = event.continuationToken?.takeIf { canSegment }?.let { token ->
            CreationContinuation(
                ownerId = prepared.ownerId,
                engineId = prepared.engineId,
                token = token,
                sourcePath = requireNotNull(request.imagePaths.singleOrNull()) {
                    "Separation source is unavailable"
                },
                outputPath = published,
                outputName = outputName,
                createdAtMs = System.currentTimeMillis(),
            )
        }
        return FinishedCreation(request, status, segmented, published, continuation)
    }

    fun recordHistory(
        completed: FinishedCreation,
        event: CreationWorkerEvent,
        protectedPaths: Set<String>,
    ) {
        val request = completed.request
        val tool = requireNotNull(CreationTool.fromWireName(request.tool))
        val metadata = creationHistoryMetadata(completed, files::presentationHandle)
        history.record(
            request.dispatchId,
            tool,
            request.imagePaths.firstOrNull()
                ?.let(files::presentationHandle).orEmpty(),
            completed.publishedPath,
            completed.status.outputName ?: request.outputName,
            metadata,
            protectedPaths,
        )
        files.prunePresentationArtifacts()
    }

}

internal fun creationHistoryMetadata(
    completed: FinishedCreation,
    presentationHandle: (String) -> String = { it },
) = buildJsonObject {
    val request = completed.request
    when (requireNotNull(CreationTool.fromWireName(request.tool))) {
        CreationTool.IMAGE_TO_3D -> {
            put("operation", request.operation)
            put("isSegmented", completed.segmented)
            request.generationMode?.let { put("generationMode", it) }
            put("polycount", request.polycount)
            put("autoSegment", request.autoSegment)
            request.instruction?.let { put("instruction", it) }
            completed.status.faces?.let { put("faces", it) }
            completed.status.vertices?.let { put("vertices", it) }
        }
        CreationTool.IMAGE_TO_SVG -> {
            put("model", request.model)
            put("backgroundMode", request.backgroundMode)
        }
        CreationTool.IMAGE_CREATOR -> {
            put("operation", request.operation)
            put("prompt", requireNotNull(request.prompt))
            put(
                "referencePreviewPaths",
                JsonArray(
                    request.imagePaths
                        .map(presentationHandle)
                        .map(::JsonPrimitive),
                ),
            )
            put("mimeType", "image/png")
            completed.status.width?.let { put("width", it) }
            completed.status.height?.let { put("height", it) }
        }
    }
}

internal fun creationJobInputPathsReleasedAfterCommit(
    request: CreationWorkerRequest,
    retainedByContinuation: Boolean = false,
): List<String> = if (retainedByContinuation) emptyList() else request.imagePaths

internal fun validatedCreationSegmentation(
    request: CreationWorkerRequest,
    event: CreationWorkerEvent,
): Boolean {
    if (request.tool != CreationTool.IMAGE_TO_3D.wireName) return false
    val productRequiresSegmented = request.operation == "segment" ||
        request.generationMode == CreationGenerationMode.FAST.wireName ||
        request.autoSegment
    require(!productRequiresSegmented || event.isSegmented != false) {
        "Creation returned conflicting model-part state"
    }
    return productRequiresSegmented || event.isSegmented == true
}

private fun Long?.boundedCreationGeometryCount(tool: CreationTool): Long? =
    takeIf {
        tool == CreationTool.IMAGE_TO_3D &&
            it != null &&
            it in 0..CreationContract.MAXIMUM_GLB_ARTIFACT_BYTES
    }
