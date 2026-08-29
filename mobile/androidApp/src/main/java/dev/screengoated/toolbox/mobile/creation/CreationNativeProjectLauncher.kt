package dev.screengoated.toolbox.mobile.creation

import android.app.Application
import java.util.UUID
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

internal class CreationNativeProjectLauncher(
    application: Application,
    private val manager: CreationJobManager,
    private val tool: CreationTool,
    private val scope: CoroutineScope,
    private val state: MutableStateFlow<CreationNativeUiState>,
    private val showError: (Throwable) -> Unit,
) {
    private val exporter = CreationProjectExporter(application, manager.files)

    fun publishChild(parent: CreationNativeItem, status: CreationJobStatus) {
        val child = parent.copy(
            id = "item_${UUID.randomUUID()}",
            batchId = status.projectId ?: parent.batchId,
            submitted = true,
            stage = CreationNativeStage.RUNNING,
            status = status,
            submissionToken = status.dispatchId,
            createdAtMs = System.currentTimeMillis(),
        )
        state.update { current ->
            current.copy(
                items = current.items + child,
                selectedItemId = child.id,
                transientError = null,
            )
        }
    }

    fun exportSelectedRevision() {
        val current = state.value
        val selected = current.selectedHistory ?: current.selectedItem?.status?.dispatchId?.let { id ->
            current.history.firstOrNull { it.dispatchId == id }
                ?: manager.history.list(tool).firstOrNull { it.dispatchId == id }
        } ?: return showError(IllegalStateException("The selected revision is not ready"))
        scope.launch(Dispatchers.IO) {
            runCatching { exporter.export(selected) }.onFailure(showError)
        }
    }
}
