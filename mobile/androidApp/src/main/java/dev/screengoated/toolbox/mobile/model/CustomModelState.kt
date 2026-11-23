package dev.screengoated.toolbox.mobile.model

import dev.screengoated.toolbox.mobile.preset.CustomPresetModelDefinition
import dev.screengoated.toolbox.mobile.preset.PresetCustomModelRegistry
import dev.screengoated.toolbox.mobile.preset.snapshotCustomModels
import dev.screengoated.toolbox.mobile.storage.CustomModelDataUnavailableException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

enum class CustomModelStatus { AVAILABLE, SAVING, DATA_UNREADABLE, WRITE_UNCERTAIN }

data class CustomModelEditorSnapshot(
    val revision: Long,
    val models: List<CustomPresetModelDefinition>,
    val status: CustomModelStatus,
    val rows: List<CustomModelRow>,
)

sealed interface CustomModelSubmission {
    data class Accepted(val snapshot: CustomModelEditorSnapshot) : CustomModelSubmission
    data class Conflict(val snapshot: CustomModelEditorSnapshot) : CustomModelSubmission
    data object Unavailable : CustomModelSubmission
}

/** A process-owned writer keeps the latest pending snapshot separate from the published catalog. */
internal class CustomModelState(
    load: () -> List<CustomPresetModelDefinition>,
    private val save: suspend (List<CustomPresetModelDefinition>) -> Unit,
    dispatcher: CoroutineDispatcher = Dispatchers.IO,
) {
    private class Write(val models: List<CustomPresetModelDefinition>)
    private val initial = try { snapshotCustomModels(load()) } catch (_: CustomModelDataUnavailableException) { null }
    private val mutableStatus = MutableStateFlow(if (initial == null) CustomModelStatus.DATA_UNREADABLE else CustomModelStatus.AVAILABLE)
    val status = mutableStatus.asStateFlow()
    val available get() = status.value in setOf(CustomModelStatus.AVAILABLE, CustomModelStatus.SAVING)
    private val mutableModels = MutableStateFlow(initial ?: emptyList())
    val models = mutableModels.asStateFlow()
    private val rows = CustomModelRows(mutableModels.value)
    private val mutableEditor = MutableStateFlow(CustomModelEditorSnapshot(0, mutableModels.value, mutableStatus.value, rows.rows))
    val editor = mutableEditor.asStateFlow()
    private var committed = initial ?: emptyList()
    private var pending: Write? = null
    private val queue = Channel<Write>(Channel.CONFLATED)
    private val scope = CoroutineScope(SupervisorJob() + dispatcher)
    private var workerStarted = false

    @Synchronized fun publish() = PresetCustomModelRegistry.set(committed)

    @Synchronized fun update(models: List<CustomPresetModelDefinition>): Boolean {
        return submit(models) != null
    }

    @Synchronized fun submit(models: List<CustomPresetModelDefinition>): CustomModelEditorSnapshot? {
        if (!available) return null
        return submitLocked(models, null)
    }

    private fun submitLocked(
        models: List<CustomPresetModelDefinition>, changedRow: CustomModelRowIdentity?,
    ): CustomModelEditorSnapshot? {
        val request = Write(snapshotCustomModels(models))
        pending = request
        mutableModels.value = request.models
        rows.reconcile(request.models, changedRow)
        val accepted = enqueue(request)
        publishEditor()
        return if (accepted) mutableEditor.value else null
    }

    @Synchronized fun submitIfCurrent(
        expected: List<CustomPresetModelDefinition>,
        models: List<CustomPresetModelDefinition>,
    ): CustomModelSubmission {
        if (!available) return CustomModelSubmission.Unavailable
        if (mutableModels.value != expected) return CustomModelSubmission.Conflict(mutableEditor.value)
        return submit(models)?.let(CustomModelSubmission::Accepted) ?: CustomModelSubmission.Unavailable
    }

    @Synchronized fun editRow(
        identity: CustomModelRowIdentity,
        transform: (CustomPresetModelDefinition) -> CustomPresetModelDefinition,
    ): CustomModelSubmission {
        if (!available) return CustomModelSubmission.Unavailable
        val index = rows.rows.indexOfFirst { it.identity == identity }
        if (index < 0) return CustomModelSubmission.Conflict(mutableEditor.value)
        val original = rows.rows[index].model
        if (rows.rows.count { it.model.id == original.id } != 1) return CustomModelSubmission.Conflict(mutableEditor.value)
        val changed = transform(original)
        if (changed.id != original.id || changed.provider != original.provider) {
            return CustomModelSubmission.Conflict(mutableEditor.value)
        }
        val next = mutableModels.value.toMutableList().also { it[index] = changed }
        return submitLocked(next, identity)?.let(CustomModelSubmission::Accepted) ?: CustomModelSubmission.Unavailable
    }

    @Synchronized fun deleteRow(identity: CustomModelRowIdentity): CustomModelSubmission {
        if (!available) return CustomModelSubmission.Unavailable
        val index = rows.rows.indexOfFirst { it.identity == identity }
        if (index < 0 || rows.rows.count { it.model.id == rows.rows[index].model.id } != 1) {
            return CustomModelSubmission.Conflict(mutableEditor.value)
        }
        val next = mutableModels.value.toMutableList().also { it.removeAt(index) }
        return submitLocked(next, null)?.let(CustomModelSubmission::Accepted) ?: CustomModelSubmission.Unavailable
    }

    @Synchronized fun retry(): Boolean {
        if (status.value != CustomModelStatus.WRITE_UNCERTAIN) return false
        val accepted = enqueue(pending ?: return false)
        publishEditor()
        return accepted
    }

    private fun publishEditor() {
        mutableEditor.value = CustomModelEditorSnapshot(
            Math.addExact(mutableEditor.value.revision, 1), mutableModels.value, mutableStatus.value, rows.rows,
        )
    }

    private fun enqueue(request: Write): Boolean {
        mutableStatus.value = CustomModelStatus.SAVING
        if (!workerStarted) {
            workerStarted = true
            scope.launch { for (write in queue) process(write) }
        }
        if (queue.trySend(request).isSuccess) return true
        mutableStatus.value = CustomModelStatus.WRITE_UNCERTAIN
        return false
    }

    private suspend fun process(write: Write) {
        val success = try {
            save(write.models)
            true
        } catch (_: CancellationException) {
            currentCoroutineContext().ensureActive()
            false
        } catch (_: Exception) {
            false
        }
        synchronized(this) {
            if (success) {
                committed = write.models
                PresetCustomModelRegistry.set(committed)
                if (pending === write) {
                    pending = null
                    mutableStatus.value = CustomModelStatus.AVAILABLE
                }
            } else {
                queue.tryReceive()
                mutableStatus.value = CustomModelStatus.WRITE_UNCERTAIN
            }
            publishEditor()
        }
    }
}
