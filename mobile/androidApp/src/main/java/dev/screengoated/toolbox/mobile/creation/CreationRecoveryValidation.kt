package dev.screengoated.toolbox.mobile.creation

import java.io.File

internal fun validateRestoredCreationRequest(
    filesDir: File,
    request: CreationWorkerRequest,
    sizeOf: (String) -> Long,
    sha256Of: (String) -> String,
): Boolean = runCatching {
    val tool = requireNotNull(CreationTool.fromWireName(request.tool))
    require(request.jobId.startsWith("${tool.wireName}_") && request.jobId.length <= 160)
    require(
        request.dispatchId.startsWith("dispatch_${tool.wireName}_") &&
            request.dispatchId.length <= 192,
    )
    require(creationRequestHasValidDeliveryIdentity(request))
    require(
        request.acceptedAtMs > 0L &&
            request.deadlineAtMs - request.acceptedAtMs ==
            CreationContract.MAXIMUM_JOB_RUNTIME_MS,
    )
    require(isReservedCreationStagingFile(filesDir, tool, request.outputPath))
    require(File(request.outputPath).name == request.outputName)
    require(request.outputName.extensionEquals(tool))
    val sources = request.imagePaths.ifEmpty {
        request.imagePath.takeIf(String::isNotBlank)?.let(::listOf).orEmpty()
    }
    require(sources == normalizeCreationImagePaths(tool, sources, null))
    require(request.sourceDescriptors.map(CreationSourceDescriptor::path) == sources)
    var aggregate = 0L
    request.sourceDescriptors.forEach { descriptor ->
        require(isManagedCreationJobInput(filesDir, request.jobId, descriptor.path))
        val actualSize = sizeOf(descriptor.path)
        require(
            actualSize == descriptor.sizeBytes &&
                actualSize in 0..CreationContract.MAXIMUM_SOURCE_IMAGE_BYTES,
        )
        require(sha256Of(descriptor.path).equals(descriptor.sha256, ignoreCase = true))
        aggregate = creationSaturatingBytes(aggregate, actualSize)
    }
    require(
        tool != CreationTool.IMAGE_CREATOR ||
            aggregate <= CreationContract.MAXIMUM_IMAGE_REFERENCE_AGGREGATE_BYTES,
    )
    require(request.hasValidProductSettings(tool))
    true
}.getOrDefault(false)

internal fun restoredCreationRecordIsBounded(
    record: CreationJournalRecord,
    nowMs: Long,
): Boolean {
    val tool = CreationTool.fromWireName(record.request.tool) ?: return false
    if (record.ownerId.isBlank() || record.ownerId.length > 160 ||
        record.ownerId.any { it.isISOControl() }
    ) return false
    if (record.destination != null &&
        (record.destination.length > 2_048 || !record.destination.startsWith("content://"))
    ) return false
    if (record.status.jobId != record.request.jobId ||
        record.startedAtMs !in 0L..(nowMs + CreationContract.MAXIMUM_JOB_RUNTIME_MS)
    ) return false
    if (record.status.operation != record.request.operation ||
        record.status.generationMode != record.request.generationMode ||
        !validateRestoredCreationSourceHandles(
            tool,
            record.status.sourceImagePaths,
            record.status.sourceImagePath,
        )
    ) return false
    if (!creationStageIsBusy(record.status.stage)) return true
    val allowedStages = when (tool) {
        CreationTool.IMAGE_TO_3D ->
            setOf("preparing", "generating", "segmenting", "refining", "finalizing")
        CreationTool.IMAGE_TO_SVG -> setOf("preparing", "generating", "finalizing")
        CreationTool.IMAGE_CREATOR -> setOf("preparing", "uploading", "generating", "finalizing")
    }
    return record.status.stage in allowedStages &&
        !(tool == CreationTool.IMAGE_CREATOR &&
            record.status.stage == "uploading" &&
            record.request.imagePaths.isEmpty())
}

internal fun validateRestoredCreationSourceHandles(
    tool: CreationTool,
    paths: List<String>,
    legacyPath: String?,
): Boolean = runCatching {
    normalizeCreationImagePaths(tool, paths, legacyPath) ==
        paths.ifEmpty { listOfNotNull(legacyPath?.takeIf(String::isNotBlank)) }
}.getOrDefault(false)

internal fun boundedRestorableCreationRecords(
    records: List<CreationJournalRecord>,
    maximumActivePerTool: Int,
    activeIsValid: (CreationJournalRecord) -> Boolean,
): List<CreationJournalRecord> {
    val activeCounts = mutableMapOf<CreationTool, Int>()
    val seenJobs = mutableSetOf<String>()
    val seenDispatches = mutableSetOf<String>()
    return records.filter { record ->
        if (!seenJobs.add(record.request.jobId)) return@filter false
        if (!creationStageIsBusy(record.status.stage)) return@filter true
        if (!seenDispatches.add(record.request.dispatchId)) return@filter false
        val tool = CreationTool.fromWireName(record.request.tool) ?: return@filter false
        val count = activeCounts.getOrDefault(tool, 0)
        if (count >= maximumActivePerTool || !activeIsValid(record)) return@filter false
        activeCounts[tool] = count + 1
        true
    }
}

private fun String.extensionEquals(tool: CreationTool): Boolean = when (tool) {
    CreationTool.IMAGE_TO_3D -> endsWith(".glb", ignoreCase = true)
    CreationTool.IMAGE_TO_SVG -> endsWith(".svg", ignoreCase = true)
    CreationTool.IMAGE_CREATOR -> endsWith(".png", ignoreCase = true)
}

private fun CreationWorkerRequest.hasValidProductSettings(tool: CreationTool): Boolean = when (tool) {
    CreationTool.IMAGE_TO_3D -> {
        val mode = CreationGenerationMode.fromWireName(generationMode)
        val route = CreationContract.route3dMode(mode, polycount, autoSegment)
        operation in setOf("generate", "segment", "refine") &&
            generationMode == mode.wireName &&
            polycount == route.polycount &&
            autoSegment == route.autoSegment &&
            (operation == "generate" ||
                (!continuationToken.isNullOrBlank() &&
                    refinementKind in CreationContract.REFINEMENT_ACTIONS))
    }
    CreationTool.IMAGE_TO_SVG -> operation == "generate" &&
        model in setOf("simple", "detail") &&
        generationMode == null &&
        !autoSegment
    CreationTool.IMAGE_CREATOR -> operation == CreationContract.IMAGE_CREATOR_OPERATION &&
        prompt?.trim()?.let {
            it.isNotEmpty() && it.length <= CreationContract.IMAGE_CREATOR_MAXIMUM_PROMPT_CHARACTERS
        } == true &&
        generationMode == null &&
        !autoSegment
}
