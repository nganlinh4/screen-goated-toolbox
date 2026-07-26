package dev.screengoated.toolbox.mobile.creation

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
        jobId: String,
    ): CreationJobDraft {
        val requestedPaths = args.strings("imagePaths")
        val legacyPath = args.string("imagePath")
        val sources = normalizeCreationImagePaths(tool, requestedPaths, legacyPath)
        require(sources.all(files::exists)) { "Image does not exist" }
        val source = sources.firstOrNull().orEmpty()
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
        val polycount = (args.int("polycount") ?: CreationContract.DEFAULT_POLYCOUNT).coerceIn(
            CreationContract.MINIMUM_POLYCOUNT,
            CreationContract.MAXIMUM_POLYCOUNT,
        )
        val requestedAutoSegment = args.boolean("autoSegment") == true &&
            args.string("segmentationMode") != "none"
        val requestedMode = CreationGenerationMode.fromWireName(args.string("generationMode"))
        val providerRoute = if (tool == CreationTool.IMAGE_TO_3D) {
            CreationContract.validate3dProvider(
                requestedMode,
                polycount,
                requestedAutoSegment,
                args.string("provider"),
            )
        } else {
            null
        }
        val output = files.stagingFile(tool, source, extension)
        val request = CreationWorkerRequest(
            jobId = jobId,
            tool = tool.wireName,
            generationMode = providerRoute?.mode?.wireName,
            provider = providerRoute?.provider?.wireName,
            operation = if (tool == CreationTool.IMAGE_CREATOR) {
                CreationContract.IMAGE_CREATOR_OPERATION
            } else {
                "generate"
            },
            imagePath = source,
            imagePaths = sources,
            prompt = prompt,
            outputPath = output.absolutePath,
            outputName = output.name,
            polycount = providerRoute?.polycount ?: polycount,
            autoSegment = providerRoute?.autoSegment ?: requestedAutoSegment,
            model = model,
        )
        return CreationJobDraft(request, initialStatus(tool, request))
    }

    fun initialStatus(tool: CreationTool, request: CreationWorkerRequest) = CreationJobStatus(
        jobId = request.jobId,
        operation = request.operation,
        generationMode = request.generationMode,
        provider = request.provider,
        polycount = request.polycount.takeIf { tool == CreationTool.IMAGE_TO_3D },
        autoSegment = request.autoSegment.takeIf { tool == CreationTool.IMAGE_TO_3D },
        stage = "preparing",
        progressText = if (tool == CreationTool.IMAGE_CREATOR) {
            "Getting ready"
        } else {
            "Preparing creation."
        },
        phase = "preparing",
        workspaceState = "checking".takeIf { tool != CreationTool.IMAGE_CREATOR },
        elapsedMs = 0,
        estimatedTotalMs = when {
            tool == CreationTool.IMAGE_CREATOR -> 180_000
            tool == CreationTool.IMAGE_TO_SVG && request.model == "detail" -> 70_000
            tool == CreationTool.IMAGE_TO_SVG -> 45_000
            request.provider == CreationProvider.MESHY.wireName -> 90_000
            request.autoSegment -> 360_000
            else -> 240_000
        },
        timingSampleCount = 0,
        progressRatio = 0.0,
        sourceImagePath = request.imagePath,
        sourceImagePaths = request.imagePaths,
        prompt = request.prompt,
        mimeType = "image/png".takeIf { tool == CreationTool.IMAGE_CREATOR },
        model = request.model.takeIf { tool == CreationTool.IMAGE_TO_SVG },
    )

    fun idleStatus(tool: CreationTool) = CreationJobStatus(
        generationMode = CreationGenerationMode.QUALITY.wireName.takeIf {
            tool == CreationTool.IMAGE_TO_3D
        },
        provider = CreationProvider.TRIPO.wireName.takeIf {
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
    val sources = (requestedPaths + listOfNotNull(legacyPath))
        .map { it.trim() }
        .filter { it.isNotEmpty() }
        .distinctBy { it.lowercase() }
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
