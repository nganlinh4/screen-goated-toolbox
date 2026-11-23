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
    val segmentationLevel: String = "detailed",
    val generationMode: String = "quality",
    val polycount: Int = CreationContract.DEFAULT_POLYCOUNT,
    val autoSegment: Boolean = false,
    val topology: String? = null,
    val instruction: String? = null,
)

internal fun creationContinuationSegmentationLevel(
    status: CreationJobStatus,
    request: CreationWorkerRequest,
): String = status.segmentationLevel ?: request.segmentationLevel

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
    val segmented: Boolean,
    val canSegment: Boolean,
    val faces: Long?,
    val vertices: Long?,
    val companionStagingPath: String? = null,
    val companionName: String? = null,
    val polygons: Long? = null,
    val quads: Long? = null,
    /** Digest of the staging bytes taken before validation; delivery must match it. */
    val validatedSha256: String? = null,
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
            // The worker shares this UID and may still hold the file open, so pin the
            // bytes before validating them; delivery re-hashes and must match.
            val validatedSha256 = files.sha256(staging.absolutePath)
            val mime = when (tool) {
                CreationTool.IMAGE_TO_3D -> {
                    CreationArtifactValidator.validateGlb(staging)
                    "model/gltf-binary"
                }
                CreationTool.IMAGE_TO_SVG -> {
                    CreationArtifactValidator.validateSvg(staging)
                    "image/svg+xml"
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
                segmented,
                canSegment,
                event.faces.boundedCreationGeometryCount(tool),
                event.vertices.boundedCreationGeometryCount(tool),
                companion?.absolutePath,
                event.downloadName,
                event.polygons.boundedCreationGeometryCount(tool),
                event.quads.boundedCreationGeometryCount(tool),
                validatedSha256 = validatedSha256,
            )
        } catch (failure: Throwable) {
            if (dev.screengoated.toolbox.mobile.BuildConfig.DEBUG) {
                android.util.Log.e("CreationCompletion", "Result validation failed", failure)
            }
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
            progressText = when (CreationTool.fromWireName(request.tool)) {
                CreationTool.IMAGE_TO_3D -> "Model ready."
                CreationTool.IMAGE_TO_SVG -> "Vector ready"
                null -> "Ready"
            },
            phase = "complete",
            progressRatio = 1.0,
            outputPath = published,
            outputName = outputName,
            downloadPath = downloadPath,
            downloadName = downloadName,
            mimeType = mime,
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
                sourcePath = request.imagePath,
                outputPath = published,
                outputName = outputName,
                createdAtMs = System.currentTimeMillis(),
                projectId = request.projectId ?: request.dispatchId,
                revisionId = request.dispatchId,
                supportedActions = supportedActions,
                availableActions = actions,
            ).withGeneration(request, status)
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
        val metadata = creationHistoryMetadata(completed)
        history.record(
            request.dispatchId,
            tool,
            request.imagePath
                .let(files::presentationHandle),
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
) = buildJsonObject {
    val request = completed.request
    when (requireNotNull(CreationTool.fromWireName(request.tool))) {
        CreationTool.IMAGE_TO_3D -> {
            put("operation", request.operation)
            put("isSegmented", completed.segmented)
            request.generationMode?.let { put("generationMode", it) }
            request.topology?.let { put("topology", it) }
            put("polycount", request.polycount)
            put("autoSegment", request.autoSegment)
            put("segmentationLevel", creationContinuationSegmentationLevel(completed.status, request))
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
    }
}

internal fun creationJobInputPathsReleasedAfterCommit(
    request: CreationWorkerRequest,
    retainedByContinuation: Boolean = false,
): List<String> = if (retainedByContinuation) emptyList() else listOf(request.imagePath)

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

internal fun validatedCreationCompanion(
    request: CreationWorkerRequest,
    event: CreationWorkerEvent,
): File? {
    if (event.downloadPath == null && event.downloadName == null) return null
    val companionOperation = request.operation in setOf("generate", "segment", "refine")
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
