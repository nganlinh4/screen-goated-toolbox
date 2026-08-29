package dev.screengoated.toolbox.mobile.creation

import android.content.Context
import android.content.Intent

internal fun shutdownCreationWorkers(
    context: Context,
    workerLock: Any,
    selected: List<Worker>,
    jobWorkers: MutableMap<String, String>,
) {
    val actions = synchronized(workerLock) {
        selected.map { worker ->
            val action = ShutdownAction(
                worker = worker,
                binder = worker.binder,
                connection = worker.connection,
                assignment = worker.assignment.lose(),
            )
            worker.binder = null
            worker.connection = null
            worker.binding = false
            worker.prepareScheduled = false
            worker.preparing = false
            worker.ready = false
            worker.busy = false
            worker.connectionEpoch += 1
            action
        }.also { shutdowns ->
            shutdowns.mapNotNull { it.assignment?.jobId }.forEach(jobWorkers::remove)
        }
    }
    actions.forEach { action ->
        action.assignment?.jobId?.let { runCatching { action.binder?.cancel(it) } }
        action.connection?.let { runCatching { context.unbindService(it) } }
        context.stopService(Intent(context, action.worker.serviceClass))
        action.assignment?.sink?.invoke(
            action.worker.key,
            CreationWorkerEvent(
                jobId = action.assignment.jobId,
                event = "execution_lost",
                failureCode = "execution_lost",
            ),
        )
    }
}
