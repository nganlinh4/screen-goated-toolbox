package dev.screengoated.toolbox.mobile.creation

import android.content.ServiceConnection
import dev.screengoated.toolbox.mobile.creation.worker.ICreationWorker

internal data class Worker(
    val key: String,
    val tool: CreationTool,
    val serviceClass: Class<*>,
    @Volatile var binder: ICreationWorker? = null,
    @Volatile var connection: ServiceConnection? = null,
    @Volatile var binding: Boolean = false,
    @Volatile var prepareScheduled: Boolean = false,
    @Volatile var preparing: Boolean = false,
    @Volatile var ready: Boolean = false,
    @Volatile var busy: Boolean = false,
    val assignment: CreationWorkerAssignmentGuard = CreationWorkerAssignmentGuard(),
    @Volatile var connectionEpoch: Long = 0,
) {
    fun preparationState() = CreationPreparationSlotState(
        connected = binder != null,
        binding = binding,
        ready = ready,
        busy = busy,
    )
}

internal data class Assignment(val worker: Worker, val binder: ICreationWorker)
internal data class PreparedCall(val binder: ICreationWorker, val epoch: Long)
internal data class WorkerLoss(
    val connection: ServiceConnection?,
    val assignment: CreationWorkerAssignment?,
)
internal data class ShutdownAction(
    val worker: Worker,
    val binder: ICreationWorker?,
    val connection: ServiceConnection?,
    val assignment: CreationWorkerAssignment?,
)
