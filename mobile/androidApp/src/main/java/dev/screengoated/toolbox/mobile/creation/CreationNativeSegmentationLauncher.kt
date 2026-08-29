package dev.screengoated.toolbox.mobile.creation

import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch

internal class CreationNativeSegmentationLauncher(
    private val manager: CreationJobManager,
    private val ownerId: String,
    private val scope: CoroutineScope,
    private val publishChild: (CreationNativeItem, CreationJobStatus) -> Unit,
    private val ensureStatusMonitor: () -> Unit,
    private val showError: (Throwable) -> Unit,
) {
    private val automaticParents = ConcurrentHashMap.newKeySet<String>()

    fun startManual(
        item: CreationNativeItem,
        kind: String = "separate_detailed",
        targetFaces: Int? = null,
        animationPreset: String? = null,
    ) = start(item, automatic = false, kind, targetFaces, animationPreset)

    fun startPending(items: List<CreationNativeItem>) {
        items.filter(::creationNeedsAutomaticSegmentation).forEach {
            start(it, automatic = true, "separate_detailed", null, null)
        }
    }

    private fun start(
        item: CreationNativeItem,
        automatic: Boolean,
        kind: String,
        targetFaces: Int?,
        animationPreset: String?,
    ) {
        val continuationId = item.status?.jobId ?: return
        if (automatic && !automaticParents.add(continuationId)) return
        scope.launch(Dispatchers.IO) {
            runCatching {
                manager.startRefinement(
                    ownerId, continuationId, kind, targetFaces, animationPreset,
                )
            }
                .onSuccess { status ->
                    publishChild(item, status)
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
