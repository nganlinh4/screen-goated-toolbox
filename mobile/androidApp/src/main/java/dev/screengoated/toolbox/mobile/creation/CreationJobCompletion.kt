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
    val projectId: String = "",
    val revisionId: String = "",
    val supportedActions: List<String>? = null,
    val availableActions: List<String> = emptyList(),
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
    val companionStagingPath: String? = null,
    val companionName: String? = null,
    val polygons: Long? = null,
    val quads: Long? = null,
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
            val companion = validatedCreationCompanion(request, event)
            val canSegment = tool == CreationTool.IMAGE_TO_3D &&
                request.operation == "generate" &&
                request.generationMode == CreationGenerationMode.QUALITY.wireName &&
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
                companion?.absolutePath,
                event.downloadName,
                event.polygons.boundedCreationGeometryCount(tool),
                event.quads.boundedCreationGeometryCount(tool),
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
        downloadPath: String? = null,
        downloadName: String? = null,
    ): FinishedCreation {
        val request = prepared.request
        val event = prepared.event
        val segmented = prepared.segmented
        val canSegment = prepared.canSegment
        val imageDimensions = prepared.imageDimensions
        val mime = prepared.mimeType
        val current = prepared.current
        val actions = event.availableActions.orEmpty()
            .filter(CreationContract.REFINEMENT_ACTIONS::contains)
            .distinct()
        val supportedActions = event.supportedActions
            ?.filter(CreationContract.REFINEMENT_ACTIONS::contains)
            ?.distinct()
            ?: actions
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
            downloadPath = downloadPath,
            downloadName = downloadName,
            mimeType = mime,
            width = imageDimensions?.width,
            height = imageDimensions?.height,
            generationMode = request.generationMode,
            isSegmented = segmented,
            canSegment = canSegment,
            projectId = request.projectId ?: request.dispatchId,
            parentRevisionId = request.parentRevisionId,
            revisionKind = request.revisionKind ?: "generated",
            supportedActions = supportedActions,
            availableActions = actions,
            isTextured = event.isTextured == true,
            isPbr = event.isPbr == true,
            isRigged = event.isRigged == true,
            rigType = event.rigType,
            canRefine = event.canRefine == true && actions.isNotEmpty(),
            faces = prepared.faces,
            vertices = prepared.vertices,
            polygons = prepared.polygons,
            quads = prepared.quads,
            error = null,
        )
        val continuation = event.continuationToken?.takeIf {
            event.canRefine == true && actions.isNotEmpty()
        }?.let { token ->
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
                projectId = request.projectId ?: request.dispatchId,
                revisionId = request.dispatchId,
                supportedActions = supportedActions,
                availableActions = actions,
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
            put("projectId", completed.status.projectId ?: request.dispatchId)
            completed.status.parentRevisionId?.let { put("parentRevisionId", it) }
            completed.status.revisionKind?.let { put("revisionKind", it) }
            put("availableActions", JsonArray(completed.status.availableActions.map(::JsonPrimitive)))
            completed.status.supportedActions?.let { actions ->
                put("supportedActions", JsonArray(actions.map(::JsonPrimitive)))
            }
            put("isTextured", completed.status.isTextured)
            put("isPbr", completed.status.isPbr)
            put("isRigged", completed.status.isRigged)
            completed.status.rigType?.let { put("rigType", it) }
            request.instruction?.let { put("instruction", it) }
            completed.status.faces?.let { put("faces", it) }
            completed.status.vertices?.let { put("vertices", it) }
            completed.status.polygons?.let { put("polygons", it) }
            completed.status.quads?.let { put("quads", it) }
            if (completed.status.downloadPath != null && completed.status.downloadName != null) {
                put(
                    "download",
                    buildJsonObject {
                        put("path", completed.status.downloadPath)
                        put("name", completed.status.downloadName)
                    },
                )
            }
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
        (request.operation == "refine" &&
            request.refinementKind?.startsWith("separate_") == true) ||
        request.generationMode == CreationGenerationMode.FAST.wireName
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

private fun validatedCreationCompanion(
    request: CreationWorkerRequest,
    event: CreationWorkerEvent,
): File? {
    if (event.downloadPath == null && event.downloadName == null) return null
    val companionOperation = request.operation == "generate" ||
        (request.operation == "refine" &&
            (request.refinementKind == "rig" ||
                request.refinementKind?.startsWith("animate_") == true))
    require(request.tool == CreationTool.IMAGE_TO_3D.wireName && companionOperation) {
        "Creation returned an unexpected companion artifact"
    }
    val expected = File(request.outputPath).absoluteFile.normalize().let { primary ->
        File(primary.parentFile, "${primary.nameWithoutExtension}.fbx")
    }
    val companion = File(requireNotNull(event.downloadPath)).absoluteFile.normalize()
    require(
        companion == expected &&
            event.downloadName == companion.name &&
            companion.isFile &&
            companion.length() in 21..CreationContract.MAXIMUM_GLB_ARTIFACT_BYTES
    ) { "Creation returned an invalid companion artifact" }
    val header = ByteArray(21)
    companion.inputStream().use { require(it.read(header) == header.size) }
    require(header.contentEquals("Kaydara FBX Binary  \u0000".toByteArray(Charsets.US_ASCII))) {
        "Creation returned an invalid companion artifact"
    }
    return companion
}
