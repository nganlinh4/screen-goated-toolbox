package dev.screengoated.toolbox.mobile.creation

import java.io.File
import java.util.UUID
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonPrimitive

internal object CreationImageSessions {
    fun new(referencePaths: List<String> = emptyList()) = CreationNativeItem(
        id = "image_${UUID.randomUUID()}",
        batchId = "batch_${UUID.randomUUID()}",
        sourcePath = referencePaths.firstOrNull().orEmpty(),
        sourceName = referencePaths.firstOrNull()?.let(::File)?.name.orEmpty(),
        referencePaths = referencePaths,
    )

    fun addReferences(
        state: CreationNativeUiState,
        paths: List<String>,
    ): CreationNativeUiState {
        val selected = state.selectedItem?.takeIf {
            !it.submitted && it.stage == CreationNativeStage.DRAFT
        } ?: new()
        val references = (selected.referencePaths + paths).distinctBy(String::lowercase)
        val maximum = CreationContract.IMAGE_CREATOR_MAXIMUM_REFERENCE_IMAGES
        val bounded = references.take(maximum)
        val updated = selected.copy(
            sourcePath = bounded.firstOrNull().orEmpty(),
            sourceName = bounded.firstOrNull()?.let(::File)?.name.orEmpty(),
            referencePaths = bounded,
        )
        val exists = state.items.any { it.id == selected.id }
        return state.copy(
            tab = CreationNativeTab.JOBS,
            items = if (exists) {
                state.items.map { if (it.id == selected.id) updated else it }
            } else {
                state.items + updated
            },
            selectedItemId = updated.id,
            selectedHistoryId = null,
            transientError = if (references.size > maximum) {
                "An image session supports up to $maximum references"
            } else {
                null
            },
        )
    }

    fun removeReference(item: CreationNativeItem, index: Int): CreationNativeItem {
        if (index !in item.referencePaths.indices) return item
        val references = item.referencePaths.toMutableList().apply { removeAt(index) }
        return item.copy(
            sourcePath = references.firstOrNull().orEmpty(),
            sourceName = references.firstOrNull()?.let(::File)?.name.orEmpty(),
            referencePaths = references,
        )
    }

    fun statusReferences(status: CreationJobStatus): List<String> =
        status.sourceImagePaths.ifEmpty {
            listOfNotNull(status.sourceImagePath?.takeIf(String::isNotBlank))
        }

    fun historyReferences(entry: CreationHistoryEntry): List<String> =
        entry.metadata["sourceImagePaths"]
            ?.jsonArray
            ?.mapNotNull { it.jsonPrimitive.contentOrNull }
            ?.take(CreationContract.IMAGE_CREATOR_MAXIMUM_REFERENCE_IMAGES)
            ?: listOfNotNull(entry.sourcePath.takeIf(String::isNotBlank))
}
