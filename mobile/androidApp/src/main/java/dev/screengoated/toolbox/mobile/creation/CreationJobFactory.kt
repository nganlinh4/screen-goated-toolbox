package dev.screengoated.toolbox.mobile.creation

import java.security.MessageDigest
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonPrimitive

internal data class CreationJobDraft(
    val request: CreationWorkerRequest,
    val status: CreationJobStatus,
)

internal object CreationJobFactory {
    fun create(
        tool: CreationTool,
        args: JsonObject,
        files: CreationFileStore,
        ownerId: String,
        jobId: String,
        dispatchId: String,
        destination: String?,
        optionalInstructionAllowed: Boolean = false,
    ): CreationJobDraft {
        val requestedPaths = args.strings("imagePaths")
        val legacyPath = args.string("imagePath")
        val sources = normalizeCreationImagePaths(tool, requestedPaths, legacyPath)
        val prompt = args.string("prompt")?.trim().orEmpty().takeIf {
            tool == CreationTool.IMAGE_CREATOR
        }
        if (tool == CreationTool.IMAGE_CREATOR) {
            require(!prompt.isNullOrBlank()) { "Describe the image you want to create" }
            require(prompt.length <= CreationContract.IMAGE_CREATOR_MAXIMUM_PROMPT_CHARACTERS) {
                "Image instructions are too long"
            }
        }
        val extension = when (tool) {
            CreationTool.IMAGE_TO_3D -> "glb"
            CreationTool.IMAGE_TO_SVG -> "svg"
            CreationTool.IMAGE_CREATOR -> "png"
        }
        val model = if (args.string("model") == "detail") "detail" else "simple"
        val backgroundMode = normalizeSvgBackgroundMode(args.string("backgroundMode"))
        val polycount = (args.int("polycount") ?: CreationContract.DEFAULT_POLYCOUNT).coerceIn(
            CreationContract.MINIMUM_POLYCOUNT,
            CreationContract.MAXIMUM_POLYCOUNT,
        )
        val requestedAutoSegment = args.boolean("autoSegment") == true &&
            args.string("segmentationMode") != "none"
        val requestedMode = CreationGenerationMode.fromWireName(args.string("generationMode"))
        val modeRoute = if (tool == CreationTool.IMAGE_TO_3D) {
            CreationContract.route3dMode(
                requestedMode,
                polycount,
                requestedAutoSegment,
            )
        } else {
            null
        }
        val instruction = normalizedCreationInstruction(
            args.string("instruction"),
            optionalInstructionAllowed,
        )
        val runtimeSources = files.materializeJobInputs(
            ownerId,
            jobId,
            sources,
            tool,
            destination,
        )
        val source = runtimeSources.firstOrNull().orEmpty()
        return try {
        val descriptors = runtimeSources.map { path ->
            CreationSourceDescriptor(
                path = path,
                sizeBytes = files.size(path).also { require(it >= 0L) { "Image is unavailable" } },
                sha256 = files.sha256(path),
            )
        }
        if (tool == CreationTool.IMAGE_CREATOR) {
            require(
                descriptors.fold(0L) { total, source ->
                    creationSaturatingBytes(total, source.sizeBytes)
                } <= CreationContract.MAXIMUM_IMAGE_REFERENCE_AGGREGATE_BYTES,
            ) { "Reference images reached the size limit" }
        }
        val output = files.stagingFile(tool, source, extension)
        val acceptedAtMs = System.currentTimeMillis()
        val unsignedRequest = CreationWorkerRequest(
            jobId = jobId,
            acceptedAtMs = acceptedAtMs,
            deadlineAtMs = acceptedAtMs + CreationContract.MAXIMUM_JOB_RUNTIME_MS,
            dispatchId = dispatchId,
            requestFingerprint = "",
            sourceDescriptors = descriptors,
            tool = tool.wireName,
            generationMode = modeRoute?.mode?.wireName,
            operation = if (tool == CreationTool.IMAGE_CREATOR) {
                CreationContract.IMAGE_CREATOR_OPERATION
            } else {
                "generate"
            },
            imagePath = source,
            imagePaths = runtimeSources,
            prompt = prompt,
            instruction = instruction,
            outputPath = output.absolutePath,
            outputName = output.name,
            polycount = modeRoute?.polycount ?: polycount,
            autoSegment = modeRoute?.autoSegment ?: false,
            model = model,
            backgroundMode = backgroundMode,
            projectId = dispatchId.takeIf { tool == CreationTool.IMAGE_TO_3D },
            revisionKind = "generated".takeIf { tool == CreationTool.IMAGE_TO_3D },
        )
        val request = unsignedRequest.copy(
            requestFingerprint = creationRequestFingerprint(unsignedRequest),
        )
        CreationJobDraft(request, initialStatus(tool, request, sources))
        } catch (failure: Throwable) {
            files.releaseJobInputs(runtimeSources)
            throw failure
        }
    }

    fun initialStatus(
        tool: CreationTool,
        request: CreationWorkerRequest,
        sourceHandles: List<String> = request.imagePaths,
    ) = CreationJobStatus(
        jobId = request.jobId,
        dispatchId = request.dispatchId,
        operation = request.operation,
        generationMode = request.generationMode,
        polycount = request.polycount.takeIf { tool == CreationTool.IMAGE_TO_3D },
        autoSegment = request.autoSegment.takeIf { tool == CreationTool.IMAGE_TO_3D },
        stage = "preparing",
        progressText = if (tool == CreationTool.IMAGE_CREATOR) {
            "Getting ready"
        } else {
            "Preparing creation."
        },
        phase = "preparing",
        elapsedMs = 0,
        estimatedTotalMs = when {
            tool == CreationTool.IMAGE_CREATOR -> 180_000
            tool == CreationTool.IMAGE_TO_SVG && request.model == "detail" -> 70_000
            tool == CreationTool.IMAGE_TO_SVG -> 45_000
            request.generationMode == CreationGenerationMode.FAST.wireName -> 90_000
            request.autoSegment -> 360_000
            else -> 240_000
        },
        timingSampleCount = 0,
        progressRatio = 0.0,
        sourceImagePath = sourceHandles.firstOrNull().orEmpty(),
        sourceImagePaths = sourceHandles,
        prompt = request.prompt,
        instruction = request.instruction,
        mimeType = "image/png".takeIf { tool == CreationTool.IMAGE_CREATOR },
        model = request.model.takeIf { tool == CreationTool.IMAGE_TO_SVG },
        backgroundMode = request.backgroundMode.takeIf { tool == CreationTool.IMAGE_TO_SVG },
        projectId = request.projectId,
        parentRevisionId = request.parentRevisionId,
        revisionKind = request.revisionKind,
    )

    fun createSegmentation(
        continuation: CreationContinuation,
        files: CreationFileStore,
        ownerId: String,
        jobId: String,
        dispatchId: String,
        destination: String?,
    ): CreationJobDraft = createRefinement(
        continuation, "separate_detailed", null, null, files, ownerId, jobId, dispatchId, destination,
    )

    fun createRefinement(
        continuation: CreationContinuation,
        refinementKind: String,
        targetFaces: Int?,
        animationPreset: String?,
        files: CreationFileStore,
        ownerId: String,
        jobId: String,
        dispatchId: String,
        destination: String?,
    ): CreationJobDraft {
        require(
            CreationContract.refinementCapability(refinementKind) in continuation.availableActions,
        ) { "This refinement is unavailable" }
        val runtimeSources = files.materializeContinuationInput(
            jobId,
            continuation.sourcePath,
            destination,
        )
        return try {
            val runtimeSource = runtimeSources.single()
            val sourceSize = files.size(runtimeSource)
            require(sourceSize >= 0L) { "The source image is unavailable" }
            val sourceDigest = files.sha256(runtimeSource)
            val output = files.stagingFile(
                CreationTool.IMAGE_TO_3D,
                runtimeSource,
                "glb",
            )
            val acceptedAtMs = System.currentTimeMillis()
            val unsigned = CreationWorkerRequest(
            jobId = jobId,
            acceptedAtMs = acceptedAtMs,
            deadlineAtMs = acceptedAtMs + CreationContract.MAXIMUM_JOB_RUNTIME_MS,
            dispatchId = dispatchId,
            requestFingerprint = "",
            sourceDescriptors = listOf(
                CreationSourceDescriptor(
                    runtimeSource,
                    sourceSize,
                    sourceDigest,
                ),
            ),
            tool = CreationTool.IMAGE_TO_3D.wireName,
            generationMode = CreationGenerationMode.QUALITY.wireName,
            operation = "refine",
            imagePath = runtimeSource,
            imagePaths = runtimeSources,
            outputPath = output.absolutePath,
            outputName = output.name,
            autoSegment = true,
            continuationToken = continuation.token,
            previousOutputPath = continuation.outputPath,
            projectId = continuation.projectId,
            parentRevisionId = continuation.revisionId,
            revisionKind = refinementKind,
            refinementKind = refinementKind,
            targetFaces = targetFaces,
            animationPreset = animationPreset,
        )
            val request = unsigned.copy(requestFingerprint = creationRequestFingerprint(unsigned))
            val status = initialStatus(
                CreationTool.IMAGE_TO_3D,
                request,
                listOf(continuation.sourcePath),
            ).copy(
                stage = "refining",
                progressText = "Creating a new version.",
                phase = "refinement",
                outputPath = continuation.outputPath,
                outputName = continuation.outputName,
            )
            CreationJobDraft(request, status)
        } catch (failure: Throwable) {
            files.releaseJobInputs(runtimeSources)
            throw failure
        }
    }

    fun idleStatus(tool: CreationTool) = CreationJobStatus(
        generationMode = CreationGenerationMode.QUALITY.wireName.takeIf {
            tool == CreationTool.IMAGE_TO_3D
        },
        operation = CreationContract.IMAGE_CREATOR_OPERATION.takeIf {
            tool == CreationTool.IMAGE_CREATOR
        },
        stage = if (tool == CreationTool.IMAGE_TO_3D) "idle" else "draft",
        progressText = "Ready to create.",
        sourceImagePath = "".takeIf { tool != CreationTool.IMAGE_TO_3D },
        prompt = "".takeIf { tool == CreationTool.IMAGE_CREATOR },
        mimeType = "image/png".takeIf { tool == CreationTool.IMAGE_CREATOR },
        model = "simple".takeIf { tool == CreationTool.IMAGE_TO_SVG },
    )
}

internal fun normalizedCreationInstruction(value: String?, allowed: Boolean): String? {
    if (!allowed) return null
    val normalized = value?.trim()?.takeIf(String::isNotEmpty) ?: return null
    require(normalized.length <= CreationContract.MAXIMUM_OPTIONAL_INSTRUCTION_CHARACTERS) {
        "The optional instruction is too long"
    }
    return normalized
}

internal fun normalizeSvgBackgroundMode(value: String?): String = when (value) {
    "auto", "transparent" -> value
    else -> "opaque"
}

private fun JsonObject.string(key: String): String? = this[key]?.jsonPrimitive?.contentOrNull
private fun JsonObject.strings(key: String): List<String> =
    this[key]?.jsonArray?.mapNotNull { it.jsonPrimitive.contentOrNull } ?: emptyList()
private fun JsonObject.int(key: String): Int? = this[key]?.jsonPrimitive?.intOrNull
private fun JsonObject.boolean(key: String): Boolean? = this[key]?.jsonPrimitive?.booleanOrNull

internal fun normalizeCreationImagePaths(
    tool: CreationTool,
    requestedPaths: List<String>,
    legacyPath: String?,
): List<String> {
    val rawSources = requestedPaths.ifEmpty { listOfNotNull(legacyPath) }
    val sources = rawSources
        .map { it.trim() }
        .filter { it.isNotEmpty() }
    return when (tool) {
        CreationTool.IMAGE_CREATOR -> sources.also {
            require(it.size <= CreationContract.IMAGE_CREATOR_MAXIMUM_REFERENCE_IMAGES) {
                "Too many reference images"
            }
        }
        CreationTool.IMAGE_TO_3D,
        CreationTool.IMAGE_TO_SVG,
        -> sources.also {
            require(it.size == 1) { "This tool accepts exactly one image" }
        }
    }

}

internal fun creationRequestFingerprint(request: CreationWorkerRequest): String {
    val canonical = Json.encodeToString(
        request.copy(
            jobId = "",
            dispatchId = "",
            requestFingerprint = "",
        ),
    )
    return MessageDigest.getInstance("SHA-256")
        .digest(canonical.encodeToByteArray())
        .joinToString("") { byte -> "%02x".format(byte) }
}

internal fun creationRequestHasValidDeliveryIdentity(request: CreationWorkerRequest): Boolean {
    val sourcePaths = request.imagePaths.ifEmpty {
        request.imagePath.takeIf(String::isNotBlank)?.let(::listOf).orEmpty()
    }
    return request.dispatchId.isNotBlank() &&
        request.dispatchId != request.jobId &&
        SHA256_HEX.matches(request.requestFingerprint) &&
        request.requestFingerprint == creationRequestFingerprint(request) &&
        request.sourceDescriptors.map(CreationSourceDescriptor::path) == sourcePaths &&
        request.sourceDescriptors.all {
            it.sizeBytes >= 0L && SHA256_HEX.matches(it.sha256)
        }
}

private val SHA256_HEX = Regex("[0-9a-f]{64}")
