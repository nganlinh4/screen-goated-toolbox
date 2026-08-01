package dev.screengoated.toolbox.mobile.creation

import kotlinx.serialization.json.Json

internal const val MAXIMUM_CREATION_WORKER_EVENT_BYTES = 64 * 1024

private val creationWorkerEventJson = Json {
    ignoreUnknownKeys = true
    encodeDefaults = true
    explicitNulls = false
}

internal fun decodeCreationWorkerEvent(eventJson: String): CreationWorkerEvent? {
    if (eventJson.length > MAXIMUM_CREATION_WORKER_EVENT_BYTES ||
        eventJson.encodeToByteArray().size > MAXIMUM_CREATION_WORKER_EVENT_BYTES
    ) {
        return null
    }
    return runCatching {
        creationWorkerEventJson.decodeFromString(CreationWorkerEvent.serializer(), eventJson)
    }.getOrNull()
}

internal fun creationPreparationEventIsReady(event: CreationWorkerEvent?): Boolean =
    event?.event == "ready" && event.ready == true

internal enum class CreationPreparationEventDisposition {
    IN_PROGRESS,
    READY,
    RETRY,
}

internal fun creationPreparationEventDisposition(
    event: CreationWorkerEvent?,
): CreationPreparationEventDisposition = when {
    event == null -> CreationPreparationEventDisposition.RETRY
    creationPreparationEventIsReady(event) -> CreationPreparationEventDisposition.READY
    event.event == "ready" || event.event == "failure" ->
        CreationPreparationEventDisposition.RETRY
    else -> CreationPreparationEventDisposition.IN_PROGRESS
}
