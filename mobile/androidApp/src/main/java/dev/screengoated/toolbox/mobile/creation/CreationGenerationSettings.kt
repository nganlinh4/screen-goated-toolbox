package dev.screengoated.toolbox.mobile.creation

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.longOrNull

internal fun newCreation3dRoute(args: JsonObject): CreationModeRoute {
    val mode = args["generationMode"]?.let {
        require(it is JsonPrimitive && it.isString) { "Generation mode is invalid" }
        it.content
    }
    val polycount = args["polycount"]?.let {
        require(it is JsonPrimitive && !it.isString) { "Polycount is invalid" }
        requireNotNull(it.longOrNull) { "Polycount is invalid" }
    } ?: CreationContract.DEFAULT_POLYCOUNT.toLong()
    val autoSegment = args["autoSegment"]?.let {
        require(it is JsonPrimitive && !it.isString) { "Automatic separation is invalid" }
        requireNotNull(it.booleanOrNull) { "Automatic separation is invalid" }
    } ?: false
    return CreationContract.route3dMode(CreationContract.newGenerationMode(mode), polycount, autoSegment)
}

internal fun CreationContinuation.withGeneration(
    request: CreationWorkerRequest,
    status: CreationJobStatus,
): CreationContinuation = copy(
    generationMode = request.generationMode ?: CreationGenerationMode.QUALITY.wireName,
    polycount = request.polycount,
    autoSegment = request.autoSegment,
    topology = request.topology,
    instruction = request.instruction,
    segmentationLevel = creationContinuationSegmentationLevel(status, request),
)

internal fun CreationWorkerRequest.withFrozenGeneration(
    continuation: CreationContinuation,
    refinementKind: String,
): CreationWorkerRequest = copy(
    generationMode = continuation.generationMode,
    polycount = continuation.polycount,
    autoSegment = continuation.autoSegment,
    topology = continuation.topology,
    instruction = continuation.instruction,
    segmentationLevel = creationRefinementSegmentationLevel(refinementKind, continuation.segmentationLevel),
)
