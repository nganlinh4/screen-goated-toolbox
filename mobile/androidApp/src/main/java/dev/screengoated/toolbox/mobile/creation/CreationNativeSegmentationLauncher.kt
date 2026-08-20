package dev.screengoated.toolbox.mobile.creation

import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch

internal class CreationNativeSegmentationLauncher(
    private val manager: CreationJobManager,
    private val ownerId: String,
    private val scope: CoroutineScope,
    private val updateItem: (String, (CreationNativeItem) -> CreationNativeItem) -> Unit,
    private val ensureStatusMonitor: () -> Unit,
    private val showError: (Throwable) -> Unit,
) {
    private val automaticParents = ConcurrentHashMap.newKeySet<String>()

    fun startManual(item: CreationNativeItem) = start(item, automatic = false)

    fun startPending(items: List<CreationNativeItem>) {
        items.filter(::creationNeedsAutomaticSegmentation).forEach {
            start(it, automatic = true)
        }
    }

    private fun start(item: CreationNativeItem, automatic: Boolean) {
        val continuationId = item.status?.jobId ?: return
        if (automatic && !automaticParents.add(continuationId)) return
        scope.launch(Dispatchers.IO) {
            runCatching { manager.startSegmentation(ownerId, continuationId) }
                .onSuccess { status ->
                    updateItem(item.id) { current ->
                        current.copy(
                            stage = CreationNativeStage.RUNNING,
                            status = status,
                            submitted = true,
                            generationMode = status.generationMode ?: current.generationMode,
                            polycount = status.polycount ?: current.polycount,
                            autoSegment = status.autoSegment ?: current.autoSegment,
                        )
                    }
                    ensureStatusMonitor()
                }
                .onFailure { failure ->
                    if (automatic) automaticParents.remove(continuationId)
                    showError(failure)
                }
        }
    }
}

internal fun creationNeedsAutomaticSegmentation(item: CreationNativeItem): Boolean =
    item.autoSegment &&
        item.stage == CreationNativeStage.DONE &&
        item.status?.canSegment == true &&
        !item.status.isSegmented
